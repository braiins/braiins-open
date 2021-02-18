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

#[macro_use]
extern crate ii_logging;

use anyhow::{anyhow, Result};
use ii_async_utils::{Spawnable, Tripwire};
use ii_stratum::v1;
use ii_wire::proxy::{self, WithProxyInfo};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    task::JoinHandle,
};

pub mod connector;
mod framing;
mod frontend;

pub use frontend::{Error, SecurityContext};

pub struct NoiseProxy {
    upstream: SocketAddr,
    security_context: Arc<SecurityContext>,
    listener: Option<TcpListener>,
    /// Builds PROXY protocol acceptor for a specified configuration
    proxy_protocol_acceptor_builder: proxy::AcceptorBuilder<TcpStream>,
    /// Server will use this version for talking to upstream server (when defined)
    _proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
}

impl NoiseProxy {
    /// `proxy_protocol_upstream_version` - If proxy protocol information is available from
    /// downstream connection this option specifies what upstream version to use
    pub async fn new<P>(
        listen_on: P,
        upstream: P,
        security_context: Arc<SecurityContext>,
        proxy_protocol_downstream_config: proxy::ProtocolConfig,
        proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
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
            proxy_protocol_acceptor_builder: proxy::AcceptorBuilder::new(
                proxy_protocol_downstream_config,
            ),
            _proxy_protocol_upstream_version: proxy_protocol_upstream_version,
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
                        self.proxy_protocol_acceptor_builder.build(tcp_stream),
                        peer_socket,
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
    proxy_protocol_acceptor: proxy::AcceptorFuture<TcpStream>,
    direct_downstream_peer_addr: SocketAddr,
    security_context: Arc<SecurityContext>,
    upstream: SocketAddr,
    tripwire: Tripwire,
) -> Result<()> {
    use futures::{sink::SinkExt, stream::StreamExt};

    let proxy_stream = proxy_protocol_acceptor.await?;
    let proxy_info = proxy_stream.proxy_info()?;

    let up_peer = upstream.to_string();

    let downstream_framed = security_context
        .build_framed_tcp_from_parts::<v1::Codec, v1::Frame, _>(proxy_stream.into_framed_parts())
        .await?;

    debug!(
        "Established secure V1 connection with {}:{}",
        direct_downstream_peer_addr, proxy_info
    );
    let upstream_framed =
        tokio_util::codec::Framed::new(TcpStream::connect(upstream).await?, v1::Codec::default());

    let (mut downstream_sink, downstream_stream) = downstream_framed.split();
    let (mut upstream_sink, upstream_stream) = upstream_framed.split();
    let tripwire1 = tripwire.clone();
    let down_to_up = async move {
        let mut str1 = downstream_stream.take_until(tripwire1);
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
        let mut str1 = upstream_stream.take_until(tripwire);

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
    debug!(
        "Session {}:{}->{} closed",
        direct_downstream_peer_addr, proxy_info, up_peer
    );
    Ok(())
}

#[cfg(test)]
mod tests {}
