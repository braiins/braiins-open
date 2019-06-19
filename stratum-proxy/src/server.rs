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
#[derive(Debug)]
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
    /// Frames from the translator to be sent out via V1 connection
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
        while let Some(msg) = await!(self.v2_conn.next()) {
            let msg = match msg {
                Ok(msg) => msg,
                Err(e) => {
                    error!(LOGGER, "Connection failed: {}", e);
                    break;
                }
            };
            msg.accept(&mut self.translation);
        }

        info!(
            LOGGER,
            "Terminating connection from: {:?}",
            self.v2_conn.peer_addr()
        )
    }
}

async fn handle_connection(mut conn_v2: Connection<V2Framing>, stratum_addr: SocketAddr) {
    let conn_v1 = match await!(Connection::connect(&stratum_addr)) {
        Ok(conn) => conn,
        Err(e) => {
            error!(LOGGER, "Connection to Stratum V1 failed: {}", e);
            return;
        }
    };

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

    let (quit_tx, mut quit_rx) = mpsc::channel(1);

    let server_task =
        async move {
            // Select over the incoming connections stream and the quit channel
            while let Some(conn) = await!(future::select(server.next(), quit_rx.next()).map(
                |either| match either {
                    Either::Left((Some(conn), _)) => Some(conn),
                    _ => None,
                }
            )) {
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
