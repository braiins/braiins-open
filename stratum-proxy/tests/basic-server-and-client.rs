//! Stratum V1/V2 Client/Server integration test that:
//! - listens on a dedicated port (9001 for V1, 9002 for V2)
//! - a sample client connects that sends and expects a response from it
//!
//! NOTE: currently this test must be run with --nocapture flag as there is no reasonable way of
//! communicating any failures/panics to the test harness.

#![feature(await_macro, async_await)]

use futures::future::Future;
use std::convert::TryInto;
use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::r#await;
use tokio::runtime::current_thread as runtime;
use wire::{tokio, Framing};

use stratum;
use stratum::test_utils;

use stratum::error::Error;
use stratum::v1::framing::codec::V1Framing;
use stratum::v1::framing::Frame::{self, RpcRequest, RpcResponse};
use stratum::v1::V1Protocol;

use stratum::v2::framing::codec::V2Framing;
use stratum::v2::framing::MessageType;

use stratumproxy::server;

use wire::utils::CompatFix;
use wire::{Connection, Payload, Server, TxFrame};

mod utils;

static ADDR: &'static str = "127.0.0.1";
static PORT_V1: usize = 9001;
static PORT_V2: usize = 9002;
static PORT_V2_FULL: usize = 9003;

#[test]
fn test_v2server() {
    // FIXME: unwraps

    tokio::run(
        async {
            let addr = format!("{}:{}", ADDR, PORT_V2).parse().unwrap();
            let mut server = Server::<V2Framing>::bind(&addr).unwrap();

            // Spawn server task that reacts to any incoming message and responds
            // with SetupMiningConnectionSuccess
            tokio::spawn_async(async move {
                let mut conn = await!(server.next()).unwrap().unwrap();
                let msg = await!(conn.next()).unwrap().unwrap();
                // test handler verifies that the message
                msg.accept(&mut test_utils::v2::TestIdentityHandler);

                // test response frame
                await!(conn.send(test_utils::v2::build_setup_mining_connection_success()));
            });

            // Testing client
            let mut connection = await!(Connection::<V2Framing>::connect(&addr))
                .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));
            await!(connection.send(test_utils::v2::build_setup_mining_connection()));

            let response = await!(connection.next()).unwrap().unwrap();
            response.accept(&mut test_utils::v2::TestIdentityHandler);
        }
            .compat_fix(),
    );
}

// WIP attempt to generalize
//fn test_server<F, P>(client_handler: &P::Handler, server_handler: &P::Handler, port: usize)
//where
//    F: wire::Framing,
//    P: wire::ProtocolBase,
//    <F as wire::Framing>::Error: std::fmt::Debug,
//    <F as wire::Framing>::Send: std::convert::From<wire::TxFrame>,
//    <F as wire::Framing>::Receive:
//{
//    tokio::run(
//        async {
//            let addr = format!("{}:{}", ADDR, port).parse().unwrap();
//
//            let mut server = Server::<F>::bind(&addr).unwrap();
//
//            // Spawn server task that reacts to any incoming message and responds
//            // with SetupMiningConnectionSuccess
//            tokio::spawn_async(async move {
//                let mut conn = await!(server.next()).unwrap().unwrap();
//                let msg:wire::Message<P> = await!(conn.next()).unwrap().unwrap();
//                // test handler verifies that the message
//                msg.accept(server_handler);
//
//                // test response frame
//                let response: TxFrame =
//                    RpcResponse(test_utils::v1::build_subscribe_ok_rpc_response())
//                        .try_into()
//                        .expect("Cannot serialize response");
//
//                await!(conn.send(response));
//            });
//
//            // Testing client
//            let mut connection =
//                await!(Connection::<F>::connect(&addr)).expect("Could not connect");
//            let request: TxFrame = RpcRequest(test_utils::v1::build_subscribe_rpc_request())
//                .try_into()
//                .expect("Cannot serialize request frame");
//            await!(connection.send(request));
//
//            let response = await!(connection.next()).unwrap().unwrap();
//            response.accept(client_handler);
//        }
//            .compat_fix(),
//    );
//}

fn v1server_task(addr: SocketAddr) -> impl Future<Output = ()> {
    let addr = format!("{}:{}", ADDR, PORT_V1).parse().unwrap();
    let mut server = Server::<V1Framing>::bind(&addr).unwrap();

    // Define a server task that reacts to any incoming message and responds
    // with SetupMiningConnectionSuccess
    async move {
        while let Some(conn) = await!(server.next()) {
            let mut conn = conn.unwrap();

            while let Some(msg) = await!(conn.next()) {
                let msg: wire::Message<V1Protocol> = msg.unwrap();
                // test handler verifies that the message
                msg.accept(&mut test_utils::v1::TestIdentityHandler);

                // test response frame
                let response = RpcResponse(test_utils::v1::build_subscribe_ok_rpc_response());
                await!(conn.send(response));
            }
        }
    }
}

#[test]
fn test_v1server() {
    runtime::run(
        async {
            let addr = format!("{}:{}", ADDR, PORT_V1).parse().unwrap();

            // Spawn server task that reacts to any incoming message and responds
            // with SetupMiningConnectionSuccess
            runtime::spawn(v1server_task(addr).compat_fix());

            // Testing client
            let mut connection = await!(Connection::<V1Framing>::connect(&addr))
                .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));

            let request = RpcRequest(test_utils::v1::build_subscribe_rpc_request());
            await!(connection.send(request));

            let response = await!(connection.next()).unwrap().unwrap();
            response.accept(&mut test_utils::v1::TestIdentityHandler);
        }
            .compat_fix(),
    );
}

#[test]
fn test_v2server_full() {
    runtime::run(
        async {
            let addr_v1 = format!("{}:{}", ADDR, PORT_V1);
            runtime::spawn(v1server_task(addr_v1.parse().unwrap()).compat_fix());

            let addr_v2 = format!("{}:{}", ADDR, PORT_V2_FULL);
            let (v2server_task, mut v2server_quit) = server::run(addr_v2.clone(), addr_v1);
            runtime::spawn(v2server_task.compat_fix());

            let sock_addr_v2 = addr_v2.parse().expect("Invalid server address");
            await!(utils::backoff(50, 4, async move || -> Result<(), Error> {
                // Testing client
                let mut conn = await!(Connection::<V2Framing>::connect(&sock_addr_v2))?;

                // Initialize server connection
                await!(conn.send(test_utils::v2::build_setup_mining_connection()));

                // let response = await!(conn.next()).unwrap().unwrap();
                // response.accept(&test_utils::v2::TestIdentityHandler);

                Ok(())
            }))
            .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr_v2, e));

            // Signal the server to shut down
            let _ = v2server_quit.try_send(());
        }
            .compat_fix(),
    );
}
