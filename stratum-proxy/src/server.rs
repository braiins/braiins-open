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

pub mod controller;
mod peer_address;

use std::convert::TryFrom;
use std::net::{SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::Arc;
use std::time;

use futures::channel::mpsc;
use futures::prelude::*;
use futures::select;
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, Instant};
use tokio_util::codec::FramedParts;

use ii_async_utils::{FutureExt, Spawnable, Tripwire};
use ii_logging::macros::*;
use ii_noise_proxy::SecurityContext;
use ii_stratum::v1;
use ii_stratum::v2;
use ii_wire::{
    proxy::{self, Connector, WithProxyInfo},
    Address, Client, Connection,
};

use crate::error::{DownstreamError, Error, Result, UpstreamError};
use crate::metrics::ProxyMetrics;
use crate::translation::V2ToV1Translation;

pub use peer_address::DownstreamPeer;

/// Represents a single protocol translation session (one V2 client talking to one V1 server)
pub struct ConnTranslation {
    /// Actual protocol translator
    translation: V2ToV1Translation,
    /// Upstream connection
    v1_conn: v1::Framed,
    /// Address of the v1 upstream peer
    v1_peer_addr: SocketAddr,
    // TODO to be removed as the translator may send out items directly via a particular connection
    // (when treated as a sink)
    /// Frames from the translator to be sent out via V1 connection
    v1_translation_rx: mpsc::Receiver<v1::Frame>,
    /// Downstream connection
    v2_conn: v2::Framed,
    /// Address of the v2 peer that has connected
    v2_peer_addr: DownstreamPeer,
    /// Frames from the translator to be sent out via V2 connection
    v2_translation_rx: mpsc::Receiver<v2::Frame>,
    metrics: Option<Arc<ProxyMetrics>>,
}

impl ConnTranslation {
    const MAX_TRANSLATION_CHANNEL_SIZE: usize = 10;
    const V1_UPSTREAM_TIMEOUT: time::Duration = time::Duration::from_secs(60);
    const V2_DOWNSTREAM_TIMEOUT: time::Duration = time::Duration::from_secs(60);

    fn new(
        v2_conn: v2::Framed,
        v2_peer_addr: DownstreamPeer,
        v1_conn: v1::Framed,
        v1_peer_addr: SocketAddr,
        metrics: Option<Arc<ProxyMetrics>>,
    ) -> Self {
        let (v1_translation_tx, v1_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let (v2_translation_tx, v2_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let translation = V2ToV1Translation::new(
            v1_translation_tx,
            v2_translation_tx,
            Default::default(),
            metrics.clone(),
        );

        Self {
            translation,
            v1_conn,
            v1_peer_addr,
            v1_translation_rx,
            v2_conn,
            v2_peer_addr,
            v2_translation_rx,
            metrics,
        }
    }

    async fn v1_handle_frame(
        translation: &mut V2ToV1Translation,
        frame: v1::framing::Frame,
    ) -> Result<()> {
        let deserialized = v1::rpc::Rpc::try_from(frame)?;
        translation.handle_v1(deserialized).await
    }

    //    async fn handle_frame(&mut self, frame: v2::framing::Frame) -> Result<()> {
    async fn v2_handle_frame(
        translation: &mut V2ToV1Translation,
        frame: v2::framing::Frame,
    ) -> Result<()> {
        match frame.header.extension_type {
            v2::extensions::BASE => {
                translation.handle_v2(frame).await?;
            }
            // Report any other extension down the line
            _ => {
                warn!("Unsupported extension frame: {:x?} ", frame);
            }
        }
        Ok(())
    }

    /// Attempt to send a frame via a specified connection. Attempt to send 'None' results in an
    /// error. The intention is to have a single place for sending out frames and handling
    /// errors/timeouts.
    pub async fn v2_try_send_frame<S>(
        connection: &mut S,
        frame: Option<v2::framing::Frame>,
        peer_addr: &DownstreamPeer,
    ) -> Result<()>
    where
        S: v2::FramedSink,
    {
        let status = match frame {
            Some(v2_translated_frame) => connection.send(v2_translated_frame).await,
            None => return Err(Error::General("No more V2 frames to send".into())),
        };
        status.map_err(|e| {
            debug!("Send error: {} for (peer: {})", e, peer_addr);
            e.into()
        })
    }

    /// Send all V2 frames via the specified V2 connection
    /// TODO consolidate this method into V2Handler, turn the parameters into fields and
    /// implement ConnTranslation::split()
    async fn v2_send_task<S>(
        mut conn_sender: S,
        mut translation_receiver: mpsc::Receiver<v2::Frame>,
        peer_addr: DownstreamPeer,
    ) -> Result<()>
    where
        S: v2::FramedSink,
    {
        loop {
            // We use select! so that more than just the translation receiver as a source can be
            // added
            select! {
                // Send out frames translated into V2
                v2_translated_frame = translation_receiver.next() => {
                    Self::v2_try_send_frame(&mut conn_sender, v2_translated_frame, &peer_addr)
                        .await?;
                },
            }
        }
    }

    async fn run(self) -> Result<()> {
        let mut v1_translation_rx = self.v1_translation_rx;
        let mut translation = self.translation;

        // TODO make connections 'optional' so that we can remove them from the instance and use
        //  the rest of the instance in as 'borrowed mutable reference'.
        let (mut v1_conn_tx, mut v1_conn_rx) = self.v1_conn.split();
        let (v2_conn_tx, mut v2_conn_rx) = self.v2_conn.split();

        // TODO factor out the frame pumping functionality and append the JoinHandle of this task
        //  to the select statement to detect any problems and to terminate the translation, too
        // V1 message send out loop
        let v1_send_task = async move {
            while let Some(frame) = v1_translation_rx.next().await {
                if let Err(err) = v1_conn_tx.send(frame).await {
                    warn!("V1 connection failed: {}", err);
                    break;
                }
            }
        };
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.accounted_spawn(v1_send_task);
            metrics.accounted_spawn(Self::v2_send_task(
                v2_conn_tx,
                self.v2_translation_rx,
                self.v2_peer_addr,
            ));
        } else {
            tokio::spawn(v1_send_task);
            tokio::spawn(Self::v2_send_task(
                v2_conn_tx,
                self.v2_translation_rx,
                self.v2_peer_addr,
            ));
        }

        // TODO: add cancel handler into the select statement
        loop {
            select! {
                // Receive V1 frame and translate it to V2 message
                v1_frame = v1_conn_rx.next().timeout(Self::V1_UPSTREAM_TIMEOUT).fuse()=> {
                    // Unwrap the potentially elapsed timeout
                    match v1_frame.map_err(|e| UpstreamError::Timeout(e))? {
                        Some(v1_frame) => {
                            Self::v1_handle_frame(
                                &mut translation,
                                v1_frame.map_err(|e| UpstreamError::Stratum(e))?,
                            )
                            .await?;
                        }
                        None => {
                            Err(format!(
                                "Upstream V1 stratum connection dropped ({:?})",
                                self.v1_peer_addr
                            ))?;
                        }
                    }
                },
                // Receive V2 frame and translate it to V1 message
                v2_frame = v2_conn_rx.next().timeout(Self::V2_DOWNSTREAM_TIMEOUT).fuse() => {
                    match v2_frame.map_err(|e| DownstreamError::Timeout(e))? {
                        Some(v2_frame) => {
                            Self::v2_handle_frame(
                                &mut translation,
                                v2_frame.map_err(|e| DownstreamError::Stratum(e))?,
                            )
                            .await?;
                        }
                        None => {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

pub trait ConnectionHandler: Clone + Send + Sync + 'static {
    fn handle_connection(
        &self,
        v2_conn: v2::Framed,
        v2_peer_addr: DownstreamPeer,
        v1_conn: v1::Framed,
        v1_peer_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;

    fn extract_proxy_proxy_info<T: WithProxyInfo>(&mut self, _connection_context: &T) {}
}

#[derive(Clone, Default)]
pub struct TranslationHandler {
    metrics: Option<Arc<ProxyMetrics>>,
}

impl TranslationHandler {
    pub fn new(metrics: Option<Arc<ProxyMetrics>>) -> Self {
        Self { metrics }
    }
}

impl ConnectionHandler for TranslationHandler {
    fn handle_connection(
        &self,
        v2_conn: v2::Framed,
        v2_peer_addr: DownstreamPeer,
        v1_conn: v1::Framed,
        v1_peer_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let translation = ConnTranslation::new(
            v2_conn,
            v2_peer_addr,
            v1_conn,
            v1_peer_addr,
            self.metrics.clone(),
        );

        translation.run().boxed()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyProtocolConfig {
    #[serde(flatten)]
    pub downstream_config: proxy::ProtocolConfig,
    /// If proxy protocol information is available from downstream connection,
    /// this option specifies what upstream version to use
    pub upstream_version: Option<proxy::ProtocolVersion>,
}

impl Default for ProxyProtocolConfig {
    fn default() -> Self {
        ProxyProtocolConfig {
            downstream_config: proxy::ProtocolConfig::new(false, vec![]),
            upstream_version: None,
        }
    }
}

struct ProxyConnection<H> {
    /// Upstream server that we should try to connect to
    v1_upstream_addr: Address,
    /// See ProxyServer
    connection_handler: H,
    /// Security context for noise handshake
    security_context: Option<Arc<SecurityContext>>,
    /// Builds PROXY protocol acceptor for a specified configuration and clones it into
    /// It is intentionally optional so that the do_handle() method can take it while working with
    /// a mutable reference of Self instance. At the same time it introduces a state into the
    /// connection, where going over `do_handle()` twice is considered a BUG.
    proxy_protocol_acceptor: Option<proxy::AcceptorFuture<TcpStream>>,
    /// Server will use this version for talking to upstream server (if any)
    proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
    metrics: Option<Arc<ProxyMetrics>>,
    client_counter: controller::ClientCounter,
    downstream_peer: DownstreamPeer,
}

impl<FN> Drop for ProxyConnection<FN> {
    fn drop(&mut self) {
        self.client_counter.decrease()
    }
}

impl<H> ProxyConnection<H>
where
    H: ConnectionHandler,
{
    fn new(
        proxy_server: &ProxyServer<H>,
        connection: TcpStream,
        downstream_peer: SocketAddr,
    ) -> Self {
        Self {
            v1_upstream_addr: proxy_server.v1_upstream_addr.clone(),
            connection_handler: proxy_server.connection_handler.clone(),
            security_context: proxy_server.security_context.clone(),
            proxy_protocol_acceptor: Some(
                proxy_server
                    .proxy_protocol_acceptor_builder
                    .build(connection),
            ),
            proxy_protocol_upstream_version: proxy_server.proxy_protocol_upstream_version,
            metrics: proxy_server.metrics.clone(),
            client_counter: proxy_server.controller.counter_for_new_client(),
            downstream_peer: DownstreamPeer::new(downstream_peer),
        }
    }

    /// Handle incoming connection:
    ///  - establish upstream V1 connection
    ///  - check PROXY protocol header (if configured)
    ///  - pass PROXY protocol header (if configured)
    ///  - establish noise handshake (if configured)
    async fn do_handle(&mut self) -> Result<()> {
        // Handle proxy protocol
        let proxy_protocol_acceptor = self
            .proxy_protocol_acceptor
            .take()
            .expect("BUG: proxy protocol acceptor has already been used");
        let proxy_stream = proxy_protocol_acceptor
            .await
            .map_err(DownstreamError::ProxyProtocol)?;
        let local_addr = proxy_stream
            .local_addr()
            .map_err(|e| DownstreamError::ProxyProtocol(ii_wire::proxy::error::Error::from(e)))?;
        let proxy_info = proxy_stream
            .proxy_info()
            .map_err(|e| DownstreamError::ProxyProtocol(e))?;
        self.downstream_peer.set_proxy_info(proxy_info);

        self.connection_handler
            .extract_proxy_proxy_info(&proxy_stream);
        debug!(
            "Received connection from downstream: {}, local destination: {}",
            self.downstream_peer,
            local_addr.to_string()
        );
        // Connect to upstream V1 server
        let mut v1_client = Client::new(self.v1_upstream_addr.clone());
        // TODO Attempt only once to connect -> consider using the backoff for a few rounds before
        // failing. Also
        // Use the connection only to build the Framed object with V1 framing and to extract the
        // peer address
        let mut v1_conn = v1_client.next().await?;
        let v1_peer_addr = v1_conn.peer_addr().map_err(UpstreamError::Io)?;

        if let Some(version) = self.proxy_protocol_upstream_version {
            let (src, dst) = if let (Some(src), Some(dst)) = (
                proxy_stream.original_peer_addr(),
                proxy_stream.original_destination_addr(),
            ) {
                (Some(src), Some(dst))
            } else {
                debug!(
                    "Passing of proxy protocol is required, but incoming connection does \
                            not contain original addresses, using socket addresses"
                );
                (Some(self.downstream_peer.direct_peer), Some(local_addr))
            };
            Connector::new(version)
                .write_proxy_header(&mut v1_conn, src, dst)
                .await
                .map_err(UpstreamError::ProxyProtocol)?;
        }
        let v1_framed_stream = Connection::<v1::Framing>::new(v1_conn).into_inner();
        debug!(
            "Established translation connection with upstream V1 {} for original V2 peer: {:?}",
            v1_peer_addr,
            proxy_stream.original_peer_addr()
        );
        let v2_framed_stream = match self.security_context.as_ref() {
            Some(security_context) => security_context
                .build_framed_tcp_from_parts(FramedParts::<
                    TcpStream,
                    ii_wire::proxy::codec::v1::V1Codec,
                >::from(proxy_stream))
                .await
                .map_err(|e| ii_stratum::error::Error::Noise(e.to_string()))?,
            None => Connection::<v2::Framing>::from(proxy_stream).into_inner(),
        };

        // Start processing of both ends
        // TODO adjust connection handler to return a Result
        self.connection_handler
            .handle_connection(
                v2_framed_stream,
                // TODO: provide connection info instead of a pure peer address, for now we just
                //  clone downstream peer
                self.downstream_peer,
                v1_framed_stream,
                v1_peer_addr,
            )
            .await
    }

    /// Handle connection by delegating it to a method that is able to handle a Result so that we
    /// have info/error reporting in a single place
    async fn handle(mut self) {
        let metrics = self.metrics.clone();
        let timer = std::time::Instant::now();
        // TODO report full address info here once ProxyConnection has internal information about
        // (possible provide full 'ProxyInfo')
        match self.do_handle().await {
            Ok(()) => {
                if let Some(x) = metrics.as_ref() {
                    x.tcp_connection_close_ok();
                }
                debug!(
                    "Connection closed by downstream peer: {}",
                    self.downstream_peer
                );
            }
            Err(err) => {
                if let Some(x) = metrics.as_ref() {
                    x.tcp_connection_close_with_error(&err);
                }
                debug!(
                    "Connection error: {} downstream peer: {}",
                    err, self.downstream_peer
                )
            }
        };
        if let Some(x) = self.metrics.as_ref() {
            x.tcp_connection_timer_observe(timer);
        }
    }
}

/// Structure representing the main server task.
///
/// Created by binding a listening socket.
/// Incoming connections are handled either by calling `next()` in a loop,
/// (a stream-like interface) or, as a higher-level interface,
/// the `run()` method turns the `ProxyServer`
/// into an asynchronous task (which internally calls `next()` in a loop).
pub struct ProxyServer<H> {
    server: Option<TcpListener>,
    listen_socket: SocketAddr,
    v1_upstream_addr: Address,
    controller: controller::Controller,
    /// Closure that generates a handler in the form of a Future that will be passed to the
    connection_handler: H,
    /// Security context for noise handshake
    security_context: Option<Arc<SecurityContext>>,
    metrics: Option<Arc<ProxyMetrics>>,
    /// Builds PROXY protocol acceptor for a specified configuration
    proxy_protocol_acceptor_builder: proxy::AcceptorBuilder<TcpStream>,
    /// Server will use this version for talking to upstream server (when defined)
    proxy_protocol_upstream_version: Option<proxy::ProtocolVersion>,
}

impl<H> ProxyServer<H>
where
    H: ConnectionHandler,
{
    /// Constructor, binds the listening socket and builds the `ProxyServer` instance with a
    /// specified `get_connection_handler` that builds the connection handler `Future` on demand
    pub async fn listen(
        listen_addr: Address,
        v1_upstream_addr: Address,
        connection_handler: H,
        security_context: Option<Arc<SecurityContext>>,
        proxy_protocol_config: ProxyProtocolConfig,
        metrics: Option<Arc<ProxyMetrics>>,
    ) -> Result<ProxyServer<H>> {
        let listen_socket = listen_addr
            .to_socket_addrs()
            .map_err(|e| Error::HostNameError(e.to_string()))?
            .next()
            .ok_or_else(|| Error::HostNameError("Failed to resolve listen_addr".into()))?;

        let mut proxy_server = ProxyServer {
            server: None,
            listen_socket,
            v1_upstream_addr,
            connection_handler,
            security_context,
            metrics,
            proxy_protocol_acceptor_builder: proxy::AcceptorBuilder::new(
                proxy_protocol_config.downstream_config,
            ),
            proxy_protocol_upstream_version: proxy_protocol_config.upstream_version,
            controller: Default::default(),
        };
        proxy_server.bind_new_socket().await?;
        Ok(proxy_server)
    }

    async fn bind_new_socket(&mut self) -> Result<()> {
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.account_tcp_listener_breakdown();
        }
        self.server.replace(
            TcpListener::bind(self.listen_socket)
                .await
                .map_err(Error::Io)?,
        );
        Ok(())
    }

    pub fn termination_notifier(&self) -> Arc<tokio::sync::Notify> {
        self.controller.termination_notifier()
    }

    /// Helper method for accepting incoming connections
    fn accept(&self, connection: TcpStream, peer: SocketAddr) -> Result<()> {
        trace!("stratum proxy: Handling connection from: {:?}", peer);
        // Fully secured connection has been established
        let proxy_connection = ProxyConnection::new(self, connection, peer);
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.accounted_spawn(proxy_connection.handle());
        } else {
            tokio::spawn(proxy_connection.handle());
        }
        Ok(())
    }

    /// Creates a proxy server task that calls `.next()`
    /// in a loop with the default error handling.
    /// The default handling simply logs all
    /// connection errors via the logging crate.
    pub async fn main_loop(mut self, tripwire: Tripwire) {
        info!(
            "Stratum proxy service starting @ {} -> {}",
            self.listen_socket, self.v1_upstream_addr
        );
        let mut inbound_conections = self
            .server
            .take()
            .expect("BUG: Missing wire::Server instance");

        let mut latest_connection_accept_failure = None::<Instant>;

        loop {
            // Three situations can happen:
            // 1. Next connection is either yielded
            // 2. Listening is terminated by tripwire (results in immediate termination)
            // 3. Listening is terminated from shutdown api call (results in slow termination)
            let tcp_accept_result = tokio::select! {
                tcp_accept_result = inbound_conections.accept() => {
                    tcp_accept_result
                },
                _ = tripwire.clone() => {
                    self.controller.request_immediate_termination();
                    break
                }
                // Termination has been requested via shutdown api
                _ = self.controller.wait_for_notification() => {
                    break
                }
            };
            match tcp_accept_result {
                Ok((stream, peer)) => {
                    debug!("Connection accepted from {}", peer);
                    if let Some(metrics) = self.metrics.as_ref() {
                        // TODO eliminate duplicate code for metrics accounting, consider moving the inc_by_error
                        //  to the caller. The problem is that it would not be as transparent due to
                        metrics.account_successful_tcp_open();
                    }
                    if let Err(err) = self.accept(stream, peer) {
                        debug!("Connection error: {}", err);
                    }
                }
                Err(accept_error) => {
                    warn!(
                        "TcpListener failed to provide functional TcpStream: {}",
                        accept_error
                    );
                    if let Some(metrics) = self.metrics.as_ref() {
                        metrics.account_unsuccessful_tcp_open();
                    }

                    if let Some(last_fail) =
                        latest_connection_accept_failure.replace(Instant::now())
                    {
                        // If the latest connection-accept event was less then millisecond ago,
                        // create drop the listener and bind again.
                        if last_fail.elapsed() < Duration::from_millis(1) {
                            info!("Trying to rebind new TcpListener");
                            // This doesn't affect existing connections
                            drop(inbound_conections);
                            // Wait a little to let system close the socket before trying to create a new one
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            inbound_conections = loop {
                                match self.bind_new_socket().await {
                                    Ok(()) => {
                                        info!("TcpListener successfully bound");
                                        break self.server.take().expect(
                                            "BUG: Missing TcpStream right after successful binding",
                                        );
                                    }
                                    Err(e) => {
                                        warn!("TcpListener cannot be bound: {}", e);
                                        tokio::time::sleep(Duration::from_millis(1000)).await;
                                    }
                                }
                            };
                        }
                    }
                }
            }
        }
        // This doesn't affect existing connections
        drop(inbound_conections);
        self.controller.wait_for_termination(None).await;

        info!("Stratum proxy service terminated");
    }
}

impl<H> Spawnable for ProxyServer<H>
where
    H: ConnectionHandler,
{
    fn run(self, tripwire: Tripwire) -> tokio::task::JoinHandle<()> {
        tokio::spawn(self.main_loop(tripwire))
    }
}
