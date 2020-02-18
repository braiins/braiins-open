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
use std::net::ToSocketAddrs;
use std::time;

use futures::channel::mpsc;
use futures::future::{self, Either};

use ii_async_compat::prelude::*;
use ii_async_compat::select;
use ii_logging::macros::*;
use ii_stratum::v1;
use ii_stratum::v2;
use ii_wire::{Connection, Server};

use crate::error::{ErrorKind, Result, ResultExt};
use crate::translation::V2ToV1Translation;

/// Represents a single protocol translation session (one V2 client talking to one V1 server)
struct ConnTranslation {
    /// Actual protocol translator
    translation: V2ToV1Translation,
    /// Upstream connection
    v1_conn: Connection<v1::Framing>,
    // TODO to be removed as the translator may send out items directly via a particular connection
    // (when treated as a sink)
    /// Frames from the translator to be sent out via V1 connection
    v1_translation_rx: mpsc::Receiver<v1::Frame>,
    /// Downstream connection
    v2_conn: Connection<v2::Framing>,
    /// Frames from the translator to be sent out via V2 connection
    v2_translation_rx: mpsc::Receiver<v2::Frame>,
}

impl ConnTranslation {
    const MAX_TRANSLATION_CHANNEL_SIZE: usize = 10;
    const V1_UPSTREAM_TIMEOUT: time::Duration = time::Duration::from_secs(60);
    const V2_DOWNSTREAM_TIMEOUT: time::Duration = time::Duration::from_secs(60);

    fn new(v2_conn: Connection<v2::Framing>, v1_conn: Connection<v1::Framing>) -> Self {
        let (v1_translation_tx, v1_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let (v2_translation_tx, v2_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let translation =
            V2ToV1Translation::new(v1_translation_tx, v2_translation_tx, Default::default());

        Self {
            translation,
            v1_conn,
            v1_translation_rx,
            v2_conn,
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
                info!(
                    "Received protocol extension frame: {:x?} passing down",
                    frame
                );
            }
        }
        Ok(())
    }

    async fn run(self) -> Result<()> {
        let mut v1_translation_rx = self.v1_translation_rx;
        let mut v2_translation_rx = self.v2_translation_rx;
        let mut translation = self.translation;
        // TODO make connections 'optional' so that we can remove them from the instance and use
        //  the rest of the instance in as 'borrowed mutable reference'.
        let (mut v1_conn_rx, mut v1_conn_tx) = self.v1_conn.split();
        let (mut v2_conn_rx, mut v2_conn_tx) = self.v2_conn.split();

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

        // V2 message send out loop
        let v2_send_task = async move {
            while let Some(frame) = v2_translation_rx.next().await {
                if let Err(err) = v2_conn_tx.send(frame).await {
                    error!("V2 connection failed: {}", err);
                    break;
                }
            }
        };
        tokio::spawn(v2_send_task);

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
                            Err("Upstream V1 stratum connection dropped")?;
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
                            Err("V2 client disconnected")?;
                        }
                    }
                }
            }
        }
    }
}

async fn handle_connection(conn_v2: Connection<v2::Framing>, stratum_addr: SocketAddr) {
    info!("Opening connection to V1: {:?}", stratum_addr);
    let conn_v1 = match Connection::connect(&stratum_addr).await {
        Ok(conn) => conn,
        Err(e) => {
            error!("Connection to Stratum V1 failed: {}", e);
            return;
        }
    };
    info!("V1 connection setup");
    // At this point, we already know the peer address is valid
    let peer_addr = conn_v2.peer_addr().expect("BUG: invalid peer address");
    let translation = ConnTranslation::new(conn_v2, conn_v1);

    if let Err(e) = translation.run().await {
        info!("Terminating connection from: {:?} ({:?})", peer_addr, e);
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
pub struct ProxyServer {
    server: Server<v2::Framing>,
    listen_addr: SocketAddr,
    stratum_addr: SocketAddr,
    quit_tx: mpsc::Sender<()>,
    quit_rx: Option<mpsc::Receiver<()>>,
}

impl ProxyServer {
    /// Constructor, binds the listening socket
    pub fn listen(listen_addr: String, stratum_addr: String) -> Result<ProxyServer> {
        let listen_addr = listen_addr
            .to_socket_addrs()
            .context(ErrorKind::BadIp(listen_addr))?
            .next()
            .expect("Cannot resolve any IP address");

        let stratum_addr = stratum_addr
            .to_socket_addrs()
            .context(ErrorKind::BadIp(stratum_addr))?
            .next()
            .expect("Cannot resolve any IP address");

        let server = Server::<v2::Framing>::bind(&listen_addr)?;

        let (quit_tx, quit_rx) = mpsc::channel(1);

        Ok(ProxyServer {
            server,
            listen_addr,
            stratum_addr,
            quit_rx: Some(quit_rx),
            quit_tx,
        })
    }

    /// Obtain the quit channel transmit end,
    /// which can be used to terminate the server task.
    pub fn quit_channel(&self) -> mpsc::Sender<()> {
        self.quit_tx.clone()
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

        let do_connect = move || {
            let conn = conn?;
            let peer_addr = conn.peer_addr()?;
            tokio::spawn(handle_connection(conn, self.stratum_addr));
            Ok(peer_addr)
        };

        Some(do_connect())
    }

    /// Creates a proxy server task that calls `.next()`
    /// in a loop with the default error handling.
    /// The default handling simply logs all
    /// connection errors via the logging crate.
    pub async fn run(mut self) {
        info!(
            "Stratum proxy service starting @ {} -> {}",
            self.listen_addr, self.stratum_addr
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
