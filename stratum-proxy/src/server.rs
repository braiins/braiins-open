use futures::channel::mpsc;
use futures::future::{self, Either, Future, FutureExt};
use futures::stream::StreamExt;
use std::net::SocketAddr;

use slog::{error, info, trace};
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::r#await;
use wire::{tokio, Framing};

use stratum;
use stratum::v1;
use stratum::v1::framing::codec::V1Framing;
use stratum::v1::{V1Handler, V1Protocol};
use stratum::v2;
use stratum::v2::framing::codec::V2Framing;
use stratum::v2::framing::MessageType;
use stratum::v2::{V2Handler, V2Protocol};
use stratum::LOGGER;

use wire::utils::CompatFix;
use wire::Message;
use wire::{Connection, Payload, Server, TxFrame};

use crate::error::*;
use crate::translation::V2ToV1Translation;

/// Represents a single protocol translation session (one V2 client talking to one V1 server)
struct ConnTranslation {
    /// Actual protocol translator
    translation: V2ToV1Translation,
    /// Upstream connection
    v1_conn: Connection<V1Framing>,
    // TODO to be removed as the translator may send out items directly via a particular connection
    // (when treated as a sink)
    /// Frames from the translator to be sent out via V1 connection
    v1_translation_rx: mpsc::Receiver<TxFrame>,
    /// Downstream connection
    v2_conn: Connection<V2Framing>,
    /// Frames from the translator to be sent out via V2 connection
    v2_translation_rx: mpsc::Receiver<TxFrame>,
}

impl ConnTranslation {
    const MAX_TRANSLATION_CHANNEL_SIZE: usize = 10;

    fn new(v2_conn: Connection<V2Framing>, v1_conn: Connection<V1Framing>) -> Self {
        let (v1_translation_tx, mut v1_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let (v2_translation_tx, mut v2_translation_rx) =
            mpsc::channel(Self::MAX_TRANSLATION_CHANNEL_SIZE);
        let mut translation = V2ToV1Translation::new(v1_translation_tx, v2_translation_tx);

        Self {
            translation,
            v1_conn,
            v1_translation_rx,
            v2_conn,
            v2_translation_rx,
        }
    }

    async fn run(mut self) {
        let mut v1_translation_rx = self.v1_translation_rx;
        let mut v2_translation_rx = self.v2_translation_rx;
        let (mut v1_conn_rx, mut v1_conn_tx) = self.v1_conn.split();
        let (mut v2_conn_rx, mut v2_conn_tx) = self.v2_conn.split();

        // V1 message send out loop
        let v1_send_task = async move {
            while let Some(msg) = await!(v1_translation_rx.next()) {
                await!(v1_conn_tx.send_async(msg));
            }
        };
        tokio::spawn(v1_send_task.compat_fix());

        // V2 message send out loop
        let v2_send_task = async move {
            while let Some(msg) = await!(v2_translation_rx.next()) {
                await!(v2_conn_tx.send_async(msg));
            }
        };
        tokio::spawn(v2_send_task.compat_fix());

        loop {
            let v1_or_v2 = await!(future::select(v1_conn_rx.next(), v2_conn_rx.next()));
            match v1_or_v2 {
                // Ok path
                Either::Left((Some(Ok(v1_msg)), _)) => v1_msg.accept(&mut self.translation),
                Either::Right((Some(Ok(v2_msg)), _)) => v2_msg.accept(&mut self.translation),

                // Connection close
                Either::Left((None, _)) | Either::Right((None, _)) => break,

                // Connection error
                Either::Left((Some(Err(err)), _)) => {
                    error!(LOGGER, "V1 connection failed: {}", err);
                    break;
                }
                Either::Right((Some(Err(err)), _)) => {
                    error!(LOGGER, "V2 connection failed: {}", err);
                    break;
                }
            }
        }

        info!(
            LOGGER,
            "Terminating connection from: {:?}",
            v2_conn_rx.peer_addr()
        )
    }
}

async fn handle_connection(mut conn_v2: Connection<V2Framing>, stratum_addr: SocketAddr) {
    info!(LOGGER, "Opening connection to V1: {:?}", stratum_addr);
    let conn_v1 = match await!(Connection::connect(&stratum_addr)) {
        Ok(conn) => conn,
        Err(e) => {
            error!(LOGGER, "Connection to Stratum V1 failed: {}", e);
            return;
        }
    };
    info!(LOGGER, "V1 connection setup");
    let translation = ConnTranslation::new(conn_v2, conn_v1);
    await!(translation.run())
}

/// Returns a server task and a channel endpoint which can be used
/// to shutdown the server task.
/// TODO: consider converting into into async and moving the binding part into the server task
pub fn run(
    listen_addr: String,
    stratum_addr: String,
) -> (impl Future<Output = ()>, mpsc::Sender<()>) {
    let listen_addr = listen_addr
        .parse()
        .expect("Failed to parse the listen address");
    let stratum_addr: SocketAddr = stratum_addr
        .parse()
        .expect("Failed to parse stratum address");
    let mut server = Server::<V2Framing>::bind(&listen_addr).expect("Failed to bind");

    info!(
        LOGGER,
        "Stratum proxy service starting @ {} -> {}", listen_addr, stratum_addr
    );

    let (quit_tx, quit_rx) = mpsc::channel(1);
    let mut quit_rx = Some(quit_rx);

    let server_task = async move {
        // Select over the incoming connections stream and the quit channel
        // In case quit_rx is closed (by quit_tx being dropped),
        // we drop quit_rx as well and switch to only awaiting the socket.
        loop {
            let conn = if let Some(quit_rx_next) = quit_rx.as_mut().map(|rx| rx.next()) {
                match await!(future::select(server.next(), quit_rx_next)) {
                    Either::Left((Some(conn), _)) => conn,
                    Either::Right((None, _)) => {
                        // The quit_rx channel has been closed / quit_tx dropped,
                        // and so we can't poll the quit_rx any more (otherwise it panics)
                        quit_rx = None;
                        continue;
                    }
                    _ => break, // Quit notification on quit_rx or socket closed
                }
            } else {
                match await!(server.next()) {
                    Some(conn) => conn,
                    None => break, // Socket closed
                }
            };

            // TODO handle connection errors
            let conn = conn.expect("Stratum error");
            info!(LOGGER, "Received a connection from: {:?}", conn.peer_addr());
            //ok_or(ErrorKind::General("V2 service terminated".into()));
            tokio::spawn(handle_connection(conn, stratum_addr).compat_fix());
        }

        info!(LOGGER, "Stratum proxy service terminated");
    };

    (server_task, quit_tx)
}
