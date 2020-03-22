// Copyright (C) 2019  Braiins Systems s.r.o.
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
use std::time;

use futures::channel::mpsc;
use futures::future::{self, Either};
use tokio::net::TcpStream;

use ii_async_compat::prelude::*;
use ii_async_compat::select;
use ii_logging::macros::*;
use ii_stratum::v1;
use ii_stratum::v2;
use ii_wire::{Address, Client, Connection, Server};

use crate::error::{ErrorKind, Result, ResultExt};
use crate::translation::V2ToV1Translation;

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
    v2_peer_addr: SocketAddr,
    /// Frames from the translator to be sent out via V2 connection
    v2_translation_rx: mpsc::Receiver<v2::Frame>,
}

impl ConnTranslation {
    const MAX_TRANSLATION_CHANNEL_SIZE: usize = 10;
    const V1_UPSTREAM_TIMEOUT: time::Duration = time::Duration::from_secs(60);
    const V2_DOWNSTREAM_TIMEOUT: time::Duration = time::Duration::from_secs(60);

    fn new(
        v2_conn: v2::Framed,
        v2_peer_addr: SocketAddr,
        v1_conn: v1::Framed,
        v1_peer_addr: SocketAddr,
    ) -> Self {
        let (v1_translation_tx, v1_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let (v2_translation_tx, v2_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let translation =
            V2ToV1Translation::new(v1_translation_tx, v2_translation_tx, Default::default());

        Self {
            translation,
            v1_conn,
            v1_peer_addr,
            v1_translation_rx,
            v2_conn,
            v2_peer_addr,
            v2_translation_rx,
        }
    }

    async fn v1_handle_frame(
        translation: &mut V2ToV1Translation,
        frame: v1::framing::Frame,
    ) -> Result<()> {
        let v1_msg = v1::build_message_from_frame(frame)?;
        v1_msg.accept(translation).await;
        Ok(())
    }

    //    async fn handle_frame(&mut self, frame: v2::framing::Frame) -> Result<()> {
    async fn v2_handle_frame(
        translation: &mut V2ToV1Translation,
        frame: v2::framing::Frame,
    ) -> Result<()> {
        match frame.header.extension_type {
            v2::extensions::BASE => {
                let event_msg = v2::build_message_from_frame(frame)?;
                event_msg.accept(translation).await;
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
        peer_addr: &SocketAddr,
    ) -> Result<()>
    where
        S: v2::FramedSink,
    {
        let status = match frame {
            Some(v2_translated_frame) => connection.send(v2_translated_frame).await,
            None => Err(ErrorKind::Io("No more frames".to_string()))?,
        };
        status.map_err(|e| {
            info!("Send error: {} for (peer: {:?})", e, peer_addr);
            e.into()
        })
    }

    /// Send all V2 frames via the specified V2 connection
    /// TODO consolidate this method into V2Handler, turn the parameters into fields and
    /// implement ConnTranslation::split()
    async fn v2_send_task<S>(
        mut conn_sender: S,
        mut translation_receiver: mpsc::Receiver<v2::Frame>,
        peer_addr: SocketAddr,
    ) -> Result<()>
    where
        S: v2::FramedSink,
    {
        loop {
            // We use select! so that more than just the translation receiver as a source can be
            // added
            select! {
                // Send out frames translated into V2
                v2_translated_frame = translation_receiver.next().fuse() => {
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
                    error!("V1 connection failed: {}", err);
                    break;
                }
            }
        };
        tokio::spawn(v1_send_task);

        tokio::spawn(Self::v2_send_task(
            v2_conn_tx,
            self.v2_translation_rx,
            self.v2_peer_addr,
        ));

        // TODO: add cancel handler into the select statement
        loop {
            select! {
                // Receive V1 frame and translate it to V2 message
                v1_frame = v1_conn_rx.next().timeout(Self::V1_UPSTREAM_TIMEOUT).fuse()=> {
                    // Unwrap the potentially elapsed timeout
                    match v1_frame? {
                        Some(v1_frame) => {
                            Self::v1_handle_frame(&mut translation, v1_frame?).await?;
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
                    match v2_frame? {
                        Some(v2_frame) => {
                            Self::v2_handle_frame(&mut translation, v2_frame?).await?;
                        }
                        None => {
                            Err(format!("V2 client disconnected ({:?})", self.v2_peer_addr))?;
                        }
                    }
                }
            }
        }
    }
}

pub async fn handle_connection(
    v2_conn: v2::Framed,
    v2_peer_addr: SocketAddr,
    v1_conn: v1::Framed,
    v1_peer_addr: SocketAddr,
) {
    let translation = ConnTranslation::new(v2_conn, v2_peer_addr, v1_conn, v1_peer_addr);

    if let Err(e) = translation.run().await {
        info!("Terminating connection from: {} ({})", v2_peer_addr, e);
    }
}

struct ProxyConnection<FN> {
    /// Downstream connection that is to be handled
    v2_downstream_conn: TcpStream,
    /// Upstream server that we should try to connect to
    v1_upstream_addr: Address,
    /// See ProxyServer
    get_connection_handler: Arc<FN>,
}

impl<FN, FT> ProxyConnection<FN>
where
    FT: Future<Output = ()>,
    FN: Fn(v2::Framed, SocketAddr, v1::Framed, SocketAddr) -> FT,
{
    fn new(
        v2_downstream_conn: TcpStream,
        v1_upstream_addr: Address,
        get_connection_handler: Arc<FN>,
    ) -> Self {
        Self {
            v2_downstream_conn,
            v1_upstream_addr,
            get_connection_handler,
        }
    }

    /// Handle incoming connection:
    ///  - establish upstream V1 connection
    ///  - establish noise handshake (if configured)
    ///  - run the custom connection handler that
    async fn do_handle(self) -> Result<SocketAddr> {
        let v2_peer_addr = self.v2_downstream_conn.peer_addr()?;

        // Connect to upstream V1 server
        let mut v1_client = Client::new(self.v1_upstream_addr);
        // TODO Attempt only once to connect -> consider using the backoff for a few rounds before
        // failing. Also
        // Use the connection only to build the Framed object with V1 framing and to extract the
        // peer address
        let v1_conn = v1_client.next().await.context("V1 upstream connection")?;
        let v1_peer_addr = v1_conn.peer_addr().context("V1 upstream peer address")?;
        let v1_framed_stream = Connection::<v1::Framing>::new(v1_conn).into_inner();
        info!(
            "Established translation connection with upstream V1 {} for V2 peer: {}",
            v1_peer_addr, v2_peer_addr
        );

        // Run the noise handshake
        // TODO: The server static key pair along with its signature (aka its "certificate")
        //  still needs to be passed to the server. This implementation doesn't prevent MITM!
        //  as nobody can trust this server yet without a signed pubkey
        let static_key =
            v2::noise::generate_keypair().expect("BUG: Failed to generate static public key");
        let responder = v2::noise::Responder::new(static_key);

        // Accept the connection and attempt to perform the full Noise handshake
        let v2_framed_stream = responder.accept(self.v2_downstream_conn).await?;

        // Start processing of both ends
        // TODO adjust connection handler to return a Result
        (self.get_connection_handler)(
            v2_framed_stream,
            v2_peer_addr,
            v1_framed_stream,
            v1_peer_addr,
        )
        .await;

        Ok(v2_peer_addr)
    }

    /// Handle connection by delegating it to a method that is able to handle a Result so that we
    /// have info/error reporting in a single place
    async fn handle(self) {
        match self.do_handle().await {
            Ok(peer) => info!("Closing connection from {} ...", peer),
            Err(err) => error!("Connection error: {}", err),
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
#[derive(Debug)]
pub struct ProxyServer<FN> {
    server: Server,
    listen_addr: Address,
    v1_upstream_addr: Address,
    quit_tx: mpsc::Sender<()>,
    quit_rx: Option<mpsc::Receiver<()>>,
    /// Closure that generates a handler in the form of a Future that will be passed to the
    get_connection_handler: Arc<FN>,
}

impl<FN, FT> ProxyServer<FN>
where
    FT: Future<Output = ()> + Send + 'static,
    FN: Fn(v2::Framed, SocketAddr, v1::Framed, SocketAddr) -> FT + Send + Sync + 'static,
{
    /// Constructor, binds the listening socket and builds the `ProxyServer` instance with a
    /// specified `get_connection_handler` that builds the connection handler `Future` on demand
    pub fn listen(
        listen_addr: Address,
        stratum_addr: Address,
        get_connection_handler: FN,
    ) -> Result<ProxyServer<FN>> {
        let server = Server::bind(&listen_addr)?;

        let (quit_tx, quit_rx) = mpsc::channel(1);

        Ok(ProxyServer {
            server,
            listen_addr,
            v1_upstream_addr: stratum_addr,
            quit_rx: Some(quit_rx),
            quit_tx,
            get_connection_handler: Arc::new(get_connection_handler),
        })
    }

    /// Obtain the quit channel transmit end,
    /// which can be used to terminate the server task.
    pub fn quit_channel(&self) -> mpsc::Sender<()> {
        self.quit_tx.clone()
    }

    /// Helper method for accepting incoming connections
    async fn accept(&self, connection_result: std::io::Result<TcpStream>) -> Result<SocketAddr> {
        let connection = connection_result?;

        let peer_addr = connection.peer_addr()?;

        // Fully secured connection has been established
        tokio::spawn(
            ProxyConnection::new(
                connection,
                self.v1_upstream_addr.clone(),
                self.get_connection_handler.clone(),
            )
            .handle(),
        );

        Ok(peer_addr)
    }

    /// Handle a connection. Call this in a loop to make the `ProxyServer`
    /// perform its job while being able to handle individual connection errors.
    ///
    /// This is a Stream-like interface but not actually implemented using a Stream
    /// because Stream doesn't get on very well with async.
    pub async fn next(&mut self) -> Option<Result<SocketAddr>> {
        // Select over the incoming connections stream and the quit channel
        // In case quit_rx is closed (by quit_tx being dropped),
        // we drop quit_rx as well and switch to only awaiting the socket.
        // Note that functional style can't really be used here because
        // unfortunately you can't await in map() et al.
        let conn = match self.quit_rx {
            Some(ref mut quit_rx) => {
                match future::select(self.server.next(), quit_rx.next()).await {
                    Either::Left((Some(conn), _)) => Some(conn),
                    Either::Right((None, _)) => {
                        // The quit_rx channel has been closed / quit_tx dropped,
                        // and so we can't poll the quit_rx any more (otherwise it panics)
                        self.quit_rx = None;
                        None
                    }
                    _ => return None, // Quit notification on quit_rx or socket closed
                }
            }
            None => None,
        };

        // If conn is None at this point, the quit_rx is no longer open
        // and we can just await the socket
        let conn = match conn {
            Some(conn) => conn,
            None => match self.server.next().await {
                Some(conn) => conn,
                None => return None, // Socket closed
            },
        };

        // Remap the connection stratum error into stratum proxy local error
        Some(self.accept(conn).await)
    }

    /// Creates a proxy server task that calls `.next()`
    /// in a loop with the default error handling.
    /// The default handling simply logs all
    /// connection errors via the logging crate.
    pub async fn run(mut self) {
        info!(
            "Stratum proxy service starting @ {} -> {}",
            self.listen_addr, self.v1_upstream_addr
        );

        while let Some(result) = self.next().await {
            match result {
                Ok(peer) => info!("Connection accepted from {}", peer),
                Err(err) => error!("Connection error: {}", err),
            }
        }

        info!("Stratum proxy service terminated");
    }
}
