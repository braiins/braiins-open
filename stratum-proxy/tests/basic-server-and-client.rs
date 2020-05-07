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

use std::convert::{TryFrom, TryInto};
use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;

use futures::prelude::*;

use ii_stratum::error::Error;
use ii_stratum::test_utils;
use ii_stratum::v1;
use ii_stratum::v2;
use ii_stratum_proxy::server;
use ii_wire::{
    Address, Connection, Server,
    {proxy, proxy::WithProxyInfo},
};
use tokio::process::Command;

mod utils;

static ADDR: &'static str = "127.0.0.1";
static PORT_V1: u16 = 9001;
const PORT_V1_FULL: u16 = 9091;
const PORT_V1_WITH_PROXY: u16 = 9092;
static PORT_V2: u16 = 9002;
static PORT_V2_FULL: u16 = 9003;
static PORT_V2_WITH_PROXY: u16 = 9004;

#[tokio::test]
async fn test_v2server() {
    let addr = Address(ADDR.into(), PORT_V2);
    let mut server = Server::bind(&addr).expect("BUG: cannot bind to address");

    // Spawn server task that reacts to any incoming message and responds
    // with SetupConnectionSuccess
    tokio::spawn(async move {
        let mut conn = Connection::<v2::Framing>::new(
            server
                .next()
                .await
                .expect("BUG: Failed to listen for a connection")
                .expect("BUG: Failed to listen for a connection"),
        );
        let frame = conn
            .next()
            .await
            .expect("BUG: Failed to read frame from Connection")
            .expect("BUG: Failed to read frame from Connection");
        // test handler verifies that the message
        test_utils::v2::TestIdentityHandler.handle_v2(frame).await;

        // test response frame
        conn.send(
            test_utils::v2::build_setup_connection_success()
                .try_into()
                .expect("BUG: Cannot convert to frame"),
        )
        .await
        .expect("BUG: Could not send message");
    });

    // Testing client
    let mut connection: Connection<v2::Framing> = addr
        .connect()
        .await
        .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e))
        .into();
    connection
        .send(
            test_utils::v2::build_setup_connection()
                .try_into()
                .expect("BUG: Cannot convert to frame"),
        )
        .await
        .expect("BUG: Could not send message");

    let response_frame = connection
        .next()
        .await
        .expect("BUG: Failed to read frame from Connection")
        .expect("BUG: Failed to read frame from Connection");
    test_utils::v2::TestIdentityHandler
        .handle_v2(response_frame)
        .await;
}

// WIP attempt to generalize
//fn test_server<F, P>(client_handler: &P::Handler, server_handler: &P::Handler, port: usize)
//where
//    F: ii_wire::Framing,
//    P: ii_wire::ProtocolBase,
//    <F as ii_wire::Framing>::Error: std::fmt::Debug,
//    <F as ii_wire::Framing>::Tx: std::convert::From<ii_wire::Frame>,
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
//                let response: Frame =
//                    RpcResponse(test_utils::v1::build_subscribe_ok_rpc_response())
//                        .try_into()
//                        .expect("Cannot serialize response");
//
//                await!(conn.send(response));
//            });
//
//            // Testing client
//            let mut connection =
//                await!(Connection::<F>::connect(addr)).expect("Could not connect");
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

fn v1server_task<A: ToSocketAddrs>(
    addr: A,
    expected_proxy_header: Option<proxy::ProxyInfo>,
) -> impl Future<Output = ()> {
    let mut server = Server::bind(&addr).expect("BUG: cannot bind to address");

    async move {
        while let Some(conn) = server.next().await {
            let conn = conn.expect("BUG server did not provide connection");
            let mut conn = match expected_proxy_header {
                None => Connection::<v1::Framing>::new(conn),
                Some(ref proxy_info) => {
                    let proxy_stream = proxy::Acceptor::new()
                        .accept(conn)
                        .await
                        .expect("Invalid proxy header");
                    // test that data in PROXY header is same as expected
                    assert_eq!(
                        proxy_info.original_destination,
                        proxy_stream.original_destination_addr()
                    );
                    assert_eq!(
                        proxy_info.original_source,
                        proxy_stream.original_peer_addr()
                    );
                    proxy_stream.into()
                }
            };

            while let Some(frame) = conn.next().await {
                let frame = frame.expect("BUG: Receiving frame failed");
                let deserialized =
                    v1::rpc::Rpc::try_from(frame).expect("BUG: Frame deserialization failed");
                // test handler verifies that the message
                test_utils::v1::TestIdentityHandler
                    .handle_v1(deserialized)
                    .await;

                // test response frame
                let response = test_utils::v1::build_subscribe_ok_response_message();
                conn.send(response.try_into().expect("BUG: Cannot convert to frame"))
                    .await
                    .expect("BUG: Could not send response");
            }
        }
    }
}

/// TODO this test is currently work in progress and is disfunctional. Code needs to be consolidated
/// And factor out common code with V2 server as attempted above.
#[tokio::test]
async fn test_v1server() {
    let addr: SocketAddr = format!("{}:{}", ADDR, PORT_V1)
        .parse()
        .expect("BUG: Failed to parse Address");

    // Spawn server task that reacts to any incoming message and responds
    // with SetupConnectionSuccess
    tokio::spawn(v1server_task(addr, None));

    // Testing client
    let mut connection = Connection::<v1::Framing>::connect(&addr)
        .await
        .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));

    let request = test_utils::v1::build_subscribe_request_frame();
    connection
        .send(request.try_into().expect("BUG: Cannot convert to frame"))
        .await
        .expect("BUG: Could not send request");

    let response_frame = connection
        .next()
        .await
        .expect("BUG: Failed to read frame from Connection")
        .expect("BUG: Failed to read frame from Connection");

    let deserialized =
        v1::rpc::Rpc::try_from(response_frame).expect("BUG: Frame deserialization failed");
    test_utils::v1::TestIdentityHandler
        .handle_v1(deserialized)
        .await;
}

async fn test_v2_client(server_addr: &Address, proxy_header: &Option<proxy::ProxyInfo>) {
    // Test client for V2
    utils::backoff(50, 4, move || {
        async move {
            let mut conn = server_addr.connect().await?;
            if let Some(proxy_header) = proxy_header {
                proxy::Connector::new()
                    .connect_to(
                        &mut conn,
                        proxy_header.original_source,
                        proxy_header.original_destination,
                    )
                    .await
                    .expect("Cannot send proxy header");
            };
            let mut conn: Connection<v2::Framing> = conn.into();

            // Initialize server connection
            conn.send(
                test_utils::v2::build_setup_connection()
                    .try_into()
                    .expect("BUG: Cannot convert to frame"),
            )
            .await
            .expect("BUG: Could not send message");

            let response = conn
                .next()
                .await
                .expect("should get response message")
                .map_err(|e| panic!("Got response error {}", e))
                .unwrap();

            test_utils::v2::TestIdentityHandler
                .handle_v2(response)
                .await;

            Result::<(), Error>::Ok(())
        }
    })
    .await
    .unwrap_or_else(|e| panic!("Could not connect to {}: {}", server_addr, e));
}

#[tokio::test]
async fn test_v2server_full_no_proxy() {
    let addr_v1 = Address(ADDR.into(), PORT_V1_FULL);
    let addr_v2 = Address(ADDR.into(), PORT_V2_FULL);

    // dummy pool server
    tokio::spawn(v1server_task(addr_v1.clone(), None));

    let v2server = server::ProxyServer::listen(
        addr_v2.clone(),
        addr_v1,
        server::handle_connection,
        None,
        (),
        server::ProxyConfig::default(),
    )
    .expect("BUG: Could not bind v2server");
    let mut v2server_quit = v2server.quit_channel();

    tokio::spawn(v2server.run());
    test_v2_client(&addr_v2, &None).await;

    // Signal the server to shut down
    let _ = v2server_quit.try_send(());
}

#[tokio::test]
async fn test_v2server_full_with_proxy() {
    let addr_v1 = Address(ADDR.into(), PORT_V1_WITH_PROXY);
    let addr_v2 = Address(ADDR.into(), PORT_V2_WITH_PROXY);

    let original_source: Option<SocketAddr> = "127.0.0.10:1234".parse().ok();
    let original_destination: Option<SocketAddr> = "127.0.0.20:5678".parse().ok();
    let proxy_info: proxy::ProxyInfo = (original_source, original_destination)
        .try_into()
        .expect("BUG: invalid addresses");

    // dummy pool server
    tokio::spawn(v1server_task(addr_v1.clone(), Some(proxy_info.clone())));

    // let v2server = server::ProxyServer::listen(
    //     addr_v2.clone(),
    //     addr_v1,
    //     server::handle_connection,
    //     None,
    //     (),
    //     server::ProxyConfig {
    //         proxy_protocol_v1: true,
    //         pass_proxy_protocol_v1: true,
    //     },
    // )
    // .expect("BUG: Could not bind v2server");
    // let mut v2server_quit = v2server.quit_channel();
    //
    // tokio::spawn(v2server.run());
    // here we prefer full integration test with running ii-stratum-proxy process
    // TODO: review if full exec is actually the desired state of the integration test
    let mut exe_file = std::env::current_exe()
        .expect("cannot get current exe path")
        .parent()
        .expect("cannot get deps dir")
        .parent()
        .expect("cannot get bin dir")
        .to_owned();
    exe_file.push("ii-stratum-proxy");
    exe_file.set_extension(std::env::consts::EXE_EXTENSION);
    assert!(exe_file.exists());
    let mut child = Command::new(exe_file)
        .arg("--proxy-protocol-v1")
        .arg("--pass-proxy-protocol-v1")
        .arg("--insecure")
        .arg("-l")
        .arg(addr_v2.to_string())
        .arg("-u")
        .arg(addr_v1.to_string())
        .spawn()
        .expect("cannot spawn proxy process");

    test_v2_client(&addr_v2, &Some(proxy_info)).await;

    // Kill proxy process
    child.kill().ok();
}
