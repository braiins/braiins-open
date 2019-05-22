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
use stratum::v1::framing::codec::V1Framing;
use stratum::v1::V1Handler;
use stratum::v2::framing::codec::V2Framing;
use stratum::v2::framing::MessageType;
use stratum::v2::V2Handler;
use stratum::LOGGER;

use wire::utils::CompatFix;
use wire::{Connection, Payload, Server, TxFrame};

use crate::error::*;

use stratum::test_utils;

#[derive(Debug)]
struct ProxyV2Handler;

impl V2Handler for ProxyV2Handler {
    // TODO
}

#[derive(Debug)]
struct Translation {
    conn_v2: Connection<V2Framing>,
    conn_v1: Connection<V1Framing>,
    handler_v2: ProxyV2Handler,
    handler_v1: ProxyV1Handler,
}

#[derive(Debug)]
struct ProxyV1Handler;

impl V1Handler for ProxyV1Handler {
    // TODO
}

impl Translation {
    fn new(conn_v2: Connection<V2Framing>, conn_v1: Connection<V1Framing>) -> Self {
        Self {
            conn_v2,
            conn_v1,
            handler_v2: ProxyV2Handler,
            handler_v1: ProxyV1Handler,
        }
    }

    async fn run(mut self) {
        while let Some(msg) = await!(self.conn_v2.next()) {
            let msg = match msg {
                Ok(msg) => msg,
                Err(e) => {
                    error!(LOGGER, "Connection failed: {}", e);
                    break;
                }
            };

            msg.accept(&self.handler_v2);
        }

        info!(
            LOGGER,
            "Terminating connection from: {:?}",
            self.conn_v2.peer_addr()
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

    let translation = Translation::new(conn_v2, conn_v1);
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
                let conn = conn.expect("Stratum error");
                info!(LOGGER, "Received a connection from: {:?}", conn.peer_addr());
                //ok_or(ErrorKind::General("V2 service terminated".into()));
                tokio::spawn(handle_connection(conn, stratum_addr).compat_fix());
            }

            info!(LOGGER, "Stratum proxy service terminated");
        };

    (server_task, quit_tx)
}
