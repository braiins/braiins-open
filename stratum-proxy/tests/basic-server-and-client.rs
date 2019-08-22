//! Stratum V1/V2 Client/Server integration test that:
//! - listens on a dedicated port (9001 for V1, 9002 for V2)
//! - a sample client connects that sends and expects a response from it
//!
//! NOTE: currently this test must be run with --nocapture flag as there is no reasonable way of
//! communicating any failures/panics to the test harness.

#![feature(await_macro, async_await)]

use futures::future::Future;
use std::net::SocketAddr;

use ii_wire::tokio;
use tokio::prelude::*;
use tokio::runtime::current_thread as runtime;

use ii_stratum::test_utils;

use ii_stratum::error::Error;
use ii_stratum::v1;
use ii_stratum::v2;

use ii_stratum_proxy::server;

use ii_wire::utils::CompatFix;
use ii_wire::{Connection, Server};

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
            let mut server = Server::<v2::Framing>::bind(&addr).unwrap();

            // Spawn server task that reacts to any incoming message and responds
            // with SetupMiningConnectionSuccess
            tokio::spawn_async(async move {
                let mut conn = await!(server.next()).unwrap().unwrap();
                let msg = await!(conn.next()).unwrap().unwrap();
                // test handler verifies that the message
                msg.accept(&mut test_utils::v2::TestIdentityHandler);

                // test response frame
                await!(conn.send(test_utils::v2::build_setup_mining_connection_success()))
                    .expect("Could not send message");
            });

            // Testing client
            let mut connection = await!(Connection::<v2::Framing>::connect(&addr))
                .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));
            await!(connection.send(test_utils::v2::build_setup_mining_connection()))
                .expect("Could not send message");

            let response = await!(connection.next()).unwrap().unwrap();
            response.accept(&mut test_utils::v2::TestIdentityHandler);
        }
            .compat_fix(),
    );
}

// WIP attempt to generalize
//fn test_server<F, P>(client_handler: &P::Handler, server_handler: &P::Handler, port: usize)
//where
//    F: ii_wire::Framing,
//    P: ii_wire::ProtocolBase,
//    <F as ii_wire::Framing>::Error: std::fmt::Debug,
//    <F as ii_wire::Framing>::Tx: std::convert::From<ii_wire::TxFrame>,
//    <F as ii_wire::Framing>::Rx:
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
//                let msg:ii_wire::Message<P> = await!(conn.next()).unwrap().unwrap();
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
    let mut server = Server::<v1::Framing>::bind(&addr).unwrap();

    async move {
        while let Some(conn) = await!(server.next()) {
            let mut conn = conn.unwrap();

            while let Some(msg) = await!(conn.next()) {
                let msg: ii_wire::Message<v1::Protocol> = msg.unwrap();
                // test handler verifies that the message
                msg.accept(&mut test_utils::v1::TestIdentityHandler);

                // test response frame
                let response = test_utils::v1::build_subscribe_ok_response_frame();
                await!(conn.send(response)).expect("Could not send response");
            }
        }
    }
}

/// TODO this test is currently work in progress and is disfunctional. Code needs to be consolidated
/// And factor out common code with V2 server as attempted above.
#[test]
#[ignore]
fn test_v1server() {
    runtime::run(
        async {
            let addr = format!("{}:{}", ADDR, PORT_V1).parse().unwrap();

            // Spawn server task that reacts to any incoming message and responds
            // with SetupMiningConnectionSuccess
            runtime::spawn(v1server_task(addr).compat_fix());

            // Testing client
            let mut connection = await!(Connection::<v1::Framing>::connect(&addr))
                .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));

            let request = test_utils::v1::build_subscribe_request_frame();
            await!(connection.send(request)).expect("Could not send request");

            let response = await!(connection.next()).unwrap().unwrap();
            response.accept(&mut test_utils::v1::TestIdentityHandler);
        }
            .compat_fix(),
    );
}

async fn test_v2_client(server_addr: String) {
    let sock_server_addr = server_addr.parse().expect("Invalid server address");
    // Test client for V2
    await!(utils::backoff(50, 4, async move || -> Result<(), Error> {
        let mut conn = await!(Connection::<v2::Framing>::connect(&sock_server_addr))?;

        // Initialize server connection
        await!(conn.send(test_utils::v2::build_setup_mining_connection()))
            .expect("Could not send message");

        // let response = await!(conn.next()).unwrap().unwrap();
        // response.accept(&test_utils::v2::TestIdentityHandler);

        Ok(())
    }))
    .unwrap_or_else(|e| panic!("Could not connect to {}: {}", server_addr, e));
}

#[test]
fn test_v2server_full() {
    runtime::run(
        async {
            // This resolves to dbg.stratum.slushpool.com
            let addr_v1 = format!("{}:{}", "52.212.249.159", 3333);
            //            let addr_v1 = format!("{}:{}", ADDR, PORT_V1);
            //            runtime::spawn(v1server_task(addr_v1.parse().unwrap()).compat_fix());

            let addr_v2 = format!("{}:{}", ADDR, PORT_V2_FULL);
            let v2server = server::ProxyServer::listen(addr_v2.clone(), addr_v1)
                .expect("Could not bind v2server");
            let mut v2server_quit = v2server.quit_channel();

            runtime::spawn(v2server.run().compat_fix());
            await!(test_v2_client(addr_v2));

            // Signal the server to shut down
            let _ = v2server_quit.try_send(());
            // TODO kill v1 test server
        }
            .compat_fix(),
    );
}
