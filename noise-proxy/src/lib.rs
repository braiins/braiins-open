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
use futures::{sink::SinkExt, stream::StreamExt};
use ii_async_utils::{Spawnable, Tripwire};
use ii_stratum::v1;
use ii_wire::proxy::{self, Connector, ProxyInfo, WithProxyInfo};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    task::JoinHandle,
};
use tokio_util::codec::{Decoder, Encoder, Framed};

pub mod connector;
mod framing;
mod frontend;
#[cfg_attr(not(feature = "prometheus_metrics"), path = "dummy_metrics.rs")]
pub mod metrics;

pub use frontend::{Error, SecurityContext};

pub struct NoiseProxy {
    upstream: SocketAddr,
    security_context: Arc<SecurityContext>,
    listener: Option<TcpListener>,
    /// Builds PROXY protocol acceptor for a specified configuration
    proxy_protocol_acceptor_builder: proxy::AcceptorBuilder<TcpStream>,
    /// Server will use this version for talking to upstream server (when defined)
    proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
    metrics: Arc<metrics::NoiseProxyMetrics>,
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
        metrics: Arc<metrics::NoiseProxyMetrics>,
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
            proxy_protocol_upstream_version,
            metrics,
        })
    }

    pub async fn main_loop(mut self, tripwire: Tripwire) {
        let listener = self.listener.take().expect("BUG: missing tcp listener");
        info!(
            "NoiseProxy: starting main loop @ {:?} -> {:?}",
            listener.local_addr(),
            self.upstream
        );
        loop {
            tokio::select! {
                tcp_accept_result = listener.accept() => {
                    let (tcp_stream, peer_socket) = match tcp_accept_result {
                        Ok(stream_and_peer) => {
                            self.metrics.account_successful_tcp_open();
                            stream_and_peer
                        }
                        Err(e) => {
                            self.metrics.account_failed_tcp_open();
                            warn!("NoiseProxy: TCP Error, disconnecting from client: {}", e);
                            // Why the sleep here?
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            continue;
                        }
                    };
                    debug!("NoiseProxy: Spawning connection task from peer {}", peer_socket);
                    let connection = NoiseProxyConnection::new(
                        self.proxy_protocol_upstream_version,
                        peer_socket,
                        self.security_context.clone(),
                        self.upstream,
                        tripwire.clone(),
                        self.metrics.clone(),
                    );
                    tokio::spawn(connection.handle(self.proxy_protocol_acceptor_builder.build(tcp_stream)));
                }
                _ = tripwire.clone() => {
                    info!("NoiseProxy: terminating");
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

struct NoiseProxyConnection {
    /// Server will use this version for talking to upstream server (when defined)
    proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
    security_context: Arc<SecurityContext>,
    direct_downstream_peer_addr: SocketAddr,
    upstream: SocketAddr,
    tripwire: Tripwire,
    metrics: Arc<metrics::NoiseProxyMetrics>,
}

impl NoiseProxyConnection {
    fn new(
        proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
        direct_downstream_peer_addr: SocketAddr,
        security_context: Arc<SecurityContext>,
        upstream: SocketAddr,
        tripwire: Tripwire,
        metrics: Arc<metrics::NoiseProxyMetrics>,
    ) -> Self {
        Self {
            proxy_protocol_upstream_version,
            security_context,
            direct_downstream_peer_addr,
            upstream,
            tripwire,
            metrics,
        }
    }

    async fn handle(self, proxy_protocol_acceptor: proxy::AcceptorFuture<TcpStream>) -> Result<()> {
        // Run the HAProxy protocol
        let proxy_stream = proxy_protocol_acceptor.await.map_err(|e| {
            self.metrics.account_tcp_close_in_stage("proxy");
            e
        })?;
        let proxy_info = proxy_stream
            .proxy_info()
            .expect("BUG: Inconsistent proxy information");
        let local_addr = proxy_stream.local_addr().map_err(|e| {
            self.metrics.account_tcp_close_in_stage("early");
            e
        })?;
        // Allows access to the peer address from both tasks
        let direct_downstream_peer_addr = self.direct_downstream_peer_addr;

        let downstream_framed = self
            .security_context
            .build_framed_tcp_from_parts::<v1::Codec, v1::Frame, _>(
                proxy_stream.into_framed_parts(),
            )
            .await
            .map_err(|e| {
                self.metrics.account_tcp_close_in_stage("downstream_noise");
                e
            })?;

        debug!(
            "NoiseProxy: Established secure V1 connection with {}:{}",
            direct_downstream_peer_addr, proxy_info
        );
        let upstream_framed = self
            .connect_upstream::<v1::Codec, v1::Frame>(proxy_info, local_addr)
            .await
            .map_err(|e| {
                self.metrics.account_tcp_close_in_stage("upstream_noise");
                e
            })?;

        let (mut downstream_sink, downstream_stream) = downstream_framed.split();
        let (mut upstream_sink, upstream_stream) = upstream_framed.split();
        let tripwire_clone = self.tripwire.clone();
        let down_to_up = async move {
            let mut str1 = downstream_stream.take_until(tripwire_clone);
            while let Some(x) = str1.next().await {
                if let Ok(frame) = x {
                    trace!("{} frame: {:?}", proxy_info, frame);
                    if let Err(e) = upstream_sink.send(frame).await {
                        warn!(
                            "NoiseProxy: {}:{} Upstream error: {}",
                            direct_downstream_peer_addr, proxy_info, e
                        );
                    }
                }
            }
            debug!(
                "NoiseProxy: Downstream disconnected: {}:{}",
                direct_downstream_peer_addr, proxy_info
            );
            if upstream_sink.close().await.is_err() {
                warn!(
                    "NoiseProxy: Error closing upstream channel for {}:{}",
                    direct_downstream_peer_addr, proxy_info
                );
            };
        };
        let tripwire_clone = self.tripwire.clone();
        let up_to_down = async move {
            let mut str1 = upstream_stream.take_until(tripwire_clone);

            while let Some(x) = str1.next().await {
                if let Ok(frame) = x {
                    trace!("{} frame: {:?}", proxy_info, frame);
                    if let Err(e) = downstream_sink.send(frame).await {
                        warn!(
                            "NoiseProxy: {}:{} Downstream error: {}",
                            direct_downstream_peer_addr, proxy_info, e
                        );
                    }
                }
            }
            debug!(
                "NoiseProxy: Upstream disconnected: {}:{}",
                direct_downstream_peer_addr, proxy_info
            );
        };
        futures::future::join(down_to_up, up_to_down).await;
        self.metrics.account_tcp_close_in_stage("ok");
        debug!(
            "NoiseProxy: Session {}:{}->{} closed",
            direct_downstream_peer_addr, proxy_info, self.upstream
        );
        Ok(())
    }

    /// Connect to upstream server and optional pass proxy information when configured to do so
    ///
    /// `proxy_info` - original source and destination addresses will be passed upstream
    /// `local_addr` - local IP address where this connection has been established is used as a
    /// failover if upstream proxy protocol is to be executed but `proxy_info` doesn't contain
    /// any useful information.
    async fn connect_upstream<C, F>(
        &self,
        proxy_info: ProxyInfo,
        local_addr: SocketAddr,
    ) -> Result<Framed<TcpStream, C>>
    where
        C: Default + Decoder + Encoder<F>,
    {
        let mut upstream = TcpStream::connect(self.upstream).await?;

        if let Some(version) = self.proxy_protocol_upstream_version {
            let (src, dst) = if let (Some(src), Some(dst)) =
                (proxy_info.original_source, proxy_info.original_destination)
            {
                (Some(src), Some(dst))
            } else {
                debug!(
                    "NoiseProxy: Passing of proxy protocol is required, but incoming connection \
                    from {}, does not contain original addresses, using socket addresses",
                    self.direct_downstream_peer_addr
                );
                (Some(self.direct_downstream_peer_addr), Some(local_addr))
            };
            Connector::new(version)
                .write_proxy_header(&mut upstream, src, dst)
                .await?;
        }
        Ok(tokio_util::codec::Framed::new(upstream, C::default()))
    }
}

#[cfg(test)]
mod tests {}
