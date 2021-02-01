// Copyright (C) 2021  Braiins Systems s.r.o.
//
// This file is part of Braiins Open-Source Initiative (BOSI).
//
// BOSI is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Please, keep in mind that we may also license BOSI or any part thereof
// under a proprietary license. For more information on the terms and conditions
// of such proprietary license or if you have any other questions, please
// contact us at opensource@braiins.com.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use ii_async_utils::{Spawnable, Tripwire};
use ii_logging::macros::*;
use ii_stratum::v1;
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    task::JoinHandle,
};

mod framing;
mod frontend;

pub use frontend::SecurityContext;

pub struct NoiseProxy {
    upstream: SocketAddr,
    security_context: Arc<SecurityContext>,
    listener: Option<TcpListener>,
}

impl NoiseProxy {
    pub async fn new<P>(
        listen_on: P,
        upstream: P,
        security_context: Arc<SecurityContext>,
    ) -> Result<Self>
    where
        P: ToSocketAddrs,
    {
        let listener = Some(TcpListener::bind(listen_on).await?);
        let upstream = tokio::net::lookup_host(upstream)
            .await?
            .next()
            .ok_or_else(|| anyhow!("Couldn't resolve upstream"))?;

        Ok(Self {
            upstream,
            security_context,
            listener,
        })
    }

    pub async fn main_loop(mut self, tripwire: Tripwire) {
        let listener = self.listener.take().expect("BUG: missing tcp listener");

        loop {
            tokio::select! {
                tcp_accept_result = listener.accept() => {
                    let (tcp_stream, peer_socket) = match tcp_accept_result {
                        Ok(x) => x,
                        Err(e) => {
                            warn!("TCP Error, disconnecting from client: {}", e);
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            continue;
                        }
                    };
                    info!("Spawning noise connection task from peer {}", peer_socket);
                    tokio::spawn(encrypt_v1_connection(
                        tcp_stream,
                        self.security_context.clone(),
                        self.upstream,
                        tripwire.clone(),
                    ));
                }
                _ = tripwire.clone() => {
                    info!("Terminating noise proxy");
                    break;
                }
            }
        }
    }
}

impl Spawnable for NoiseProxy {
    fn run(self, tripwire: Tripwire) -> JoinHandle<()> {
        tokio::spawn(self.main_loop(tripwire))
    }
}

async fn encrypt_v1_connection(
    tcp_stream: TcpStream,
    security_context: Arc<SecurityContext>,
    upstream: SocketAddr,
    tripwire: Tripwire,
) -> Result<()> {
    use futures::{sink::SinkExt, stream::StreamExt};

    let down_peer = tcp_stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "??".to_owned());
    let up_peer = upstream.to_string();

    let downstream_framed = security_context
        .build_framed_tcp::<v1::Codec, v1::Frame>(tcp_stream)
        .await?;

    debug!("Established secure V1 connection with {}", down_peer);
    let upstream_framed =
        tokio_util::codec::Framed::new(TcpStream::connect(upstream).await?, v1::Codec::default());

    let (mut downstream_sink, mut downstream_stream) = downstream_framed.split();
    let (mut upstream_sink, mut upstream_stream) = upstream_framed.split();
    let tripwire1 = tripwire.clone();
    let down_to_up = async move {
        let mut str1 = futures::StreamExt::take_until(&mut downstream_stream, tripwire1);
        while let Some(x) = str1.next().await {
            if let Ok(frame) = x {
                if let Err(e) = upstream_sink.send(frame).await {
                    warn!("Upstream error: {}", e);
                } else {
                    trace!("-> Frame")
                }
            }
        }
        info!("Downstream disconnected");
        if upstream_sink.close().await.is_err() {
            warn!("Error closing upstream channel");
        };
    };
    let up_to_down = async move {
        let mut str1 = futures::StreamExt::take_until(&mut upstream_stream, tripwire);

        while let Some(x) = str1.next().await {
            if let Ok(frame) = x {
                if let Err(e) = downstream_sink.send(frame).await {
                    warn!("Error: {}", e);
                } else {
                    trace!("<- Frame")
                }
            }
        }
        info!("Upstream disconnected");
    };
    futures::future::join(down_to_up, up_to_down).await;
    debug!("Session {}->{} closed", down_peer, up_peer);
    Ok(())
}

#[cfg(test)]
mod tests {}
