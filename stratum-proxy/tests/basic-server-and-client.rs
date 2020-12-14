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

use async_trait::async_trait;
use futures::prelude::*;
use std::convert::{TryFrom, TryInto};
use std::net::{SocketAddr, ToSocketAddrs};

use ii_stratum::error::Error;
use ii_stratum::test_utils;
use ii_stratum::test_utils::v1::TestFrameReceiver as _;
use ii_stratum::v1;
use ii_stratum::v2;
use ii_stratum_proxy::server;
use ii_wire::{
    Address, Connection, Server,
    {proxy, proxy::WithProxyInfo},
};

mod utils;

static ADDR: &'static str = "127.0.0.1";
static PORT_V1: u16 = 9001;
const PORT_V1_FULL: u16 = 9091;
const PORT_V1_WITH_PROXY: u16 = 9092;
static PORT_V2: u16 = 9002;
static PORT_V2_FULL: u16 = 9003;
static PORT_V2_WITH_PROXY: u16 = 9004;

/// Generic stratum V1 tester that is able to send and receive a V1 frame and can be used for
/// verifying client and server protocol flows
struct StratumV1Tester {
    conn: Connection<v1::Framing>,
}

impl StratumV1Tester {
    fn new(conn: Connection<v1::Framing>) -> Self {
        Self { conn }
    }

    pub async fn send_v1<M>(&mut self, message: M)
    where
        M: TryInto<v1::Frame, Error = ii_stratum::error::Error>,
    {
        // create a tx frame, we won't send it but only extract the pure data
        // (as it implements the deref trait) as if it arrived to translation
        let frame: v1::Frame = message.try_into().expect("BUG: Deserialization failed");
        //let rpc = v1::rpc::Rpc::try_from(frame).expect("BUG: Message deserialization failed");

        self.conn
            .send(frame)
            .await
            .expect("BUG: V1 Frame sending failed");
    }
}

#[async_trait]
impl test_utils::v1::TestFrameReceiver for StratumV1Tester {
    async fn receive_v1(&mut self) -> v1::rpc::Rpc {
        let frame = self
            .conn
            .next()
            .await
            .expect("BUG: At least 1 message was expected")
            .expect("BUG: Failed to receive a V1 frame");

        v1::rpc::Rpc::try_from(frame).expect("BUG: Message deserialization failed")
    }
}

/// TODO consolidate the server to use StratumV2Tester (implement StratumV1Tester equivalent)
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

fn v1server_task<A: ToSocketAddrs>(
    addr: A,
    expected_proxy_header: Option<proxy::ProxyInfo>,
) -> impl Future<Output = ()> {
    let mut server = Server::bind(&addr).expect("BUG: cannot bind to address");

    async move {
        while let Some(conn) = server.next().await {
            let conn = conn.expect("BUG: server did not provide connection");
            let conn = match expected_proxy_header {
                None => Connection::<v1::Framing>::new(conn),
                Some(ref proxy_info) => {
                    let proxy_stream = proxy::Acceptor::new()
                        .accept_auto(conn)
                        .await
                        .expect("BUG: Invalid proxy header");
                    // Test that data in PROXY header matches
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
            let mut stratum_v1_tester = StratumV1Tester::new(conn);
            let id = 0.into();
            stratum_v1_tester
                .check_next_v1(id, |msg: v1::messages::Configure| {
                    test_utils::v1::message_request_check(
                        id,
                        &msg,
                        test_utils::v1::build_configure(),
                        test_utils::v1::MINING_CONFIGURE_REQ_JSON,
                    );
                })
                .await;
            stratum_v1_tester
                .send_v1(test_utils::v1::build_configure_ok_response_message())
                .await;
            let id = 1.into();
            stratum_v1_tester
                .check_next_v1(id, |msg: v1::messages::Subscribe| {
                    test_utils::v1::message_request_check(
                        id,
                        &msg,
                        test_utils::v1::build_subscribe(),
                        test_utils::v1::MINING_SUBSCRIBE_REQ_JSON,
                    );
                })
                .await;
            stratum_v1_tester
                .send_v1(test_utils::v1::build_subscribe_ok_response_message())
                .await;
        }
    }
}

/// Verify functionality of stratum V1 server
#[tokio::test]
async fn test_v1server() {
    let addr: SocketAddr = format!("{}:{}", ADDR, PORT_V1)
        .parse()
        .expect("BUG: Failed to parse Address");

    // Spawn server task that reacts to any incoming message and responds
    // with SetupConnectionSuccess
    tokio::spawn(v1server_task(addr, None));

    // Testing client
    let connection = Connection::<v1::Framing>::connect(&addr)
        .await
        .unwrap_or_else(|e| panic!("Could not connect to {}: {}", addr, e));
    let mut stratum_v1_tester = StratumV1Tester::new(connection);

    let id = 0.into();
    stratum_v1_tester
        .send_v1(test_utils::v1::build_configure_request())
        .await;
    stratum_v1_tester
        .check_next_v1(id, |msg: v1::messages::ConfigureResult| {
            test_utils::v1::message_check(
                id,
                &msg,
                test_utils::v1::build_configure_ok_result(),
                test_utils::v1::build_configure_ok_response_message(),
                test_utils::v1::MINING_CONFIGURE_OK_RESP_JSON,
            );
        })
        .await;

    let id = 1.into();
    stratum_v1_tester
        .send_v1(test_utils::v1::build_subscribe_request_frame())
        .await;
    stratum_v1_tester
        .check_next_v1(id, |msg: v1::messages::SubscribeResult| {
            test_utils::v1::message_check(
                id,
                &msg,
                test_utils::v1::build_subscribe_ok_result(),
                test_utils::v1::build_subscribe_ok_response_message(),
                test_utils::v1::MINING_SUBSCRIBE_OK_RESULT_JSON,
            );
        })
        .await;
}

/// Helper V2 client for testing the stratum proxy with or without proxy protocol
/// TODO: use StratumV2Tester once it is available (equivalent of StratumV1Tester)
async fn test_v2_client(server_addr: &Address, proxy_proto_info: &Option<proxy::ProxyInfo>) {
    // Test client for V2
    utils::backoff(50, 4, move || {
        async move {
            let mut conn = server_addr.connect().await?;
            if let Some(proxy_proto_info) = proxy_proto_info {
                proxy::Connector::new(proxy::ProtocolVersion::V2)
                    .write_proxy_header(
                        &mut conn,
                        proxy_proto_info.original_source,
                        proxy_proto_info.original_destination,
                    )
                    .await
                    .expect("BUG: Cannot send proxy header");
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
                .expect("BUG: should get response message")
                .expect("BUG: failed to get response");

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
async fn test_v2server_full_no_proxy_protocol() {
    let addr_v1 = Address(ADDR.into(), PORT_V1_FULL);
    let addr_v2 = Address(ADDR.into(), PORT_V2_FULL);

    // dummy pool server
    tokio::spawn(v1server_task(addr_v1.clone(), None));

    let v2server = server::ProxyServer::listen(
        addr_v2.clone(),
        addr_v1,
        server::TranslationHandler::new(None),
        None,
        server::ProxyProtocolConfig {
            downstream_config: proxy::ProtocolConfig::new(false, vec![]),
            upstream_version: None,
        },
        None,
    )
    .expect("BUG: Could not bind v2server");
    let mut v2server_quit = v2server.quit_channel();

    tokio::spawn(v2server.run());
    test_v2_client(&addr_v2, &None).await;

    // Signal the server to shut down
    let _ = v2server_quit.try_send(());
}

#[tokio::test]
async fn test_v2server_full_with_proxy_protocol() {
    let addr_v1 = Address(ADDR.into(), PORT_V1_WITH_PROXY);
    let addr_v2 = Address(ADDR.into(), PORT_V2_WITH_PROXY);

    let original_source: Option<SocketAddr> = "127.0.0.10:1234".parse().ok();
    let original_destination: Option<SocketAddr> = "127.0.0.20:5678".parse().ok();
    let proxy_info: proxy::ProxyInfo = (original_source, original_destination)
        .try_into()
        .expect("BUG: invalid addresses");

    // Dummy pool server
    tokio::spawn(v1server_task(addr_v1.clone(), Some(proxy_info.clone())));

    let v2server = server::ProxyServer::listen(
        addr_v2.clone(),
        addr_v1,
        server::TranslationHandler::new(None),
        None,
        server::ProxyProtocolConfig {
            downstream_config: proxy::ProtocolConfig::new(
                false,
                vec![proxy::ProtocolVersion::V1, proxy::ProtocolVersion::V2],
            ),
            upstream_version: Some(proxy::ProtocolVersion::V2),
        },
        None,
    )
    .expect("BUG: Could not bind v2server");
    let mut v2server_quit = v2server.quit_channel();
    tokio::spawn(v2server.run());

    test_v2_client(&addr_v2, &Some(proxy_info)).await;

    // Signal the server to shut down
    let _ = v2server_quit.try_send(());
}
