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

use ii_wire::{Connection, Server};

mod utils;

static ADDR: &'static str = "127.0.0.1";
static PORT_V1: usize = 9001;
static PORT_V2: usize = 9002;
static PORT_V2_FULL: usize = 9003;

#[test]
fn test_v2server() {
    // FIXME: unwraps

    ii_async_compat::run(async {
        let addr = format!("{}:{}", ADDR, PORT_V2).parse().unwrap();
        let mut server = Server::<v2::Framing>::bind(&addr).unwrap();

        // Spawn server task that reacts to any incoming message and responds
        // with SetupConnectionSuccess
        tokio::spawn_async(async move {
            let mut conn = await!(server.next()).unwrap().unwrap();
            let msg = await!(conn.next()).unwrap().unwrap();
            // test handler verifies that the message
            msg.accept(&mut test_utils::v2::TestIdentityHandler);

            // test response frame
            await!(conn.send(test_utils::v2::build_setup_connection_success()))
                .expect("Could not send message");
        });

        // Testing client
        let mut connection = await!(Connection::<v2::Framing>::connect(&addr))
            .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));
        await!(connection.send(test_utils::v2::build_setup_connection()))
            .expect("Could not send message");

        let response = await!(connection.next()).unwrap().unwrap();
        response.accept(&mut test_utils::v2::TestIdentityHandler);
    });
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
//    ii_async_compat::run(
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
    ii_async_compat::run(async {
        let addr = format!("{}:{}", ADDR, PORT_V1).parse().unwrap();

        // Spawn server task that reacts to any incoming message and responds
        // with SetupConnectionSuccess
        ii_async_compat::spawn(v1server_task(addr));

        // Testing client
        let mut connection = await!(Connection::<v1::Framing>::connect(&addr))
            .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));

        let request = test_utils::v1::build_subscribe_request_frame();
        await!(connection.send(request)).expect("Could not send request");

        let response = await!(connection.next()).unwrap().unwrap();
        response.accept(&mut test_utils::v1::TestIdentityHandler);
    });
}

async fn test_v2_client(server_addr: String) {
    let sock_server_addr = server_addr.parse().expect("Invalid server address");
    // Test client for V2
    await!(utils::backoff(50, 4, async move || -> Result<(), Error> {
        let mut conn = await!(Connection::<v2::Framing>::connect(&sock_server_addr))?;

        // Initialize server connection
        await!(conn.send(test_utils::v2::build_setup_connection()))
            .expect("Could not send message");

        // let response = await!(conn.next()).unwrap().unwrap();
        // response.accept(&test_utils::v2::TestIdentityHandler);

        Ok(())
    }))
    .unwrap_or_else(|e| panic!("Could not connect to {}: {}", server_addr, e));
}

#[test]
fn test_v2server_full() {
    ii_async_compat::run(async {
        // This resolves to dbg.stratum.slushpool.com
        let addr_v1 = format!("{}:{}", "52.212.249.159", 3333);
        //            let addr_v1 = format!("{}:{}", ADDR, PORT_V1);
        //            ii_async_compat::spawn(v1server_task(addr_v1.parse().unwrap()));

        let addr_v2 = format!("{}:{}", ADDR, PORT_V2_FULL);
        let v2server =
            server::ProxyServer::listen(addr_v2.clone(), addr_v1).expect("Could not bind v2server");
        let mut v2server_quit = v2server.quit_channel();

        ii_async_compat::spawn(v2server.run());
        await!(test_v2_client(addr_v2));

        // Signal the server to shut down
        let _ = v2server_quit.try_send(());
        // TODO kill v1 test server
    });
}
