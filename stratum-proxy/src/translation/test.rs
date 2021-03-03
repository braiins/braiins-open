// Copyright (C) 2021  Braiins Systems s.r.o.
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

use async_trait::async_trait;
use std::iter::repeat;

use futures::stream::StreamExt;
use primitive_types::U256;

use super::*;
use ii_stratum::test_utils;
use ii_stratum::test_utils::v1::TestFrameReceiver as _;
use ii_stratum::test_utils::v2::TestFrameReceiver as _;
use ii_stratum::v1;
use ii_stratum::v2;

struct TranslationTester {
    translation: V2ToV1Translation,
    v1_receiver: mpsc::Receiver<v1::Frame>,
    v2_receiver: mpsc::Receiver<v2::Frame>,
}

impl TranslationTester {
    pub fn new(options: V2ToV1TranslationOptions) -> Self {
        let (v1_sender, v1_receiver) = mpsc::channel(1);
        let (v2_sender, v2_receiver) = mpsc::channel(1);
        let translation =
            V2ToV1Translation::new(v1_sender, v2_sender, options, None, Default::default());

        Self {
            translation,
            v1_receiver,
            v2_receiver,
        }
    }

    pub async fn send_v1<M>(&mut self, message: M)
    where
        M: TryInto<v1::Frame, Error = ii_stratum::error::Error>,
    {
        // create a tx frame, we won't send it but only extract the pure data
        // (as it implements the deref trait) as if it arrived to translation
        let frame: v1::Frame = message.try_into().expect("BUG: Deserialization failed");
        let rpc = v1::rpc::Rpc::try_from(frame).expect("BUG: Message deserialization failed");

        self.translation
            .handle_v1(rpc)
            .await
            .expect("BUG: V1 Frame handling failed");
    }

    /// Simulates incoming message by converting it into a `Frame` and running the deserialization
    /// chain from that point on
    pub async fn send_v2<M>(&mut self, message: M)
    where
        M: TryInto<v2::Frame, Error = ii_stratum::error::Error>,
    {
        // create a tx frame, we won't send it but only extract the pure data
        // (as it implements the deref trait)
        let frame: v2::Frame = message
            .try_into()
            .expect("BUG: Could not serialize message");

        self.translation
            .handle_v2(frame)
            .await
            .expect("BUG: Message handling failed");
    }
}

impl Default for TranslationTester {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

#[async_trait]
impl test_utils::v1::TestFrameReceiver for TranslationTester {
    async fn receive_v1(&mut self) -> v1::rpc::Rpc {
        let frame = self
            .v1_receiver
            .next()
            .await
            .expect("BUG: At least 1 message was expected");

        v1::rpc::Rpc::try_from(frame).expect("BUG: Message deserialization failed")
    }
}

#[async_trait]
impl test_utils::v2::TestFrameReceiver for TranslationTester {
    async fn receive_v2(&mut self) -> v2::framing::Frame {
        self.v2_receiver
            .next()
            .await
            .expect("BUG: At least 1 message was expected")
    }
}

#[tokio::test]
async fn test_client_reconnect_translate() {
    let mut tr_options = V2ToV1TranslationOptions::default();
    tr_options.propagate_reconnect_downstream = true;
    let mut tester = TranslationTester::new(tr_options);

    tester
        .send_v1(test_utils::v1::build_client_reconnect_request_message())
        .await;
    tester
        .check_next_v2(|msg: v2::messages::Reconnect| {
            test_utils::v2::message_check(msg, test_utils::v2::build_reconnect());
        })
        .await;
}

async fn test_initial_sequence_translate(tester: &mut TranslationTester) {
    // Setup mining connection should result into: mining.configure
    tester
        .send_v2(test_utils::v2::build_setup_connection())
        .await;
    let id = 0.into();
    tester
        .check_next_v1(id, |msg: v1::messages::Configure| {
            test_utils::v1::message_request_check(
                id,
                &msg,
                test_utils::v1::build_configure(),
                test_utils::v1::MINING_CONFIGURE_REQ_JSON,
            );
        })
        .await;

    tester
        .send_v1(test_utils::v1::build_configure_ok_response_message())
        .await;
    tester
        .check_next_v2(|msg: v2::messages::SetupConnectionSuccess| {
            test_utils::v2::message_check(msg, test_utils::v2::build_setup_connection_success());
        })
        .await;

    // Opening a channel should result into: V1 generating a subscribe and authorize requests
    tester.send_v2(test_utils::v2::build_open_channel()).await;
    let id = 1.into();
    tester
        .check_next_v1(id, |msg: v1::messages::Subscribe| {
            test_utils::v1::message_request_check(
                id,
                &msg,
                test_utils::v1::build_subscribe(),
                test_utils::v1::MINING_SUBSCRIBE_REQ_JSON,
            );
        })
        .await;
    let id = 2.into();
    tester
        .check_next_v1(id, |msg: v1::messages::Authorize| {
            test_utils::v1::message_request_check(
                id,
                &msg,
                test_utils::v1::build_authorize(),
                test_utils::v1::MINING_AUTHORIZE_JSON,
            );
        })
        .await;

    // Subscribe response
    tester
        .send_v1(test_utils::v1::build_subscribe_ok_response_message())
        .await;
    // Authorize response
    tester
        .send_v1(test_utils::v1::build_authorize_ok_response_message())
        .await;

    // SetDifficulty notification before completion
    tester
        .send_v1(test_utils::v1::build_set_difficulty_request_message())
        .await;
    // Now we should have a successfully open channel
    tester
        .check_next_v2(|msg: v2::messages::OpenStandardMiningChannelSuccess| {
            test_utils::v2::message_check(msg, test_utils::v2::build_open_channel_success());
        })
        .await;

    tester
        .send_v1(test_utils::v1::build_mining_notify_request_message())
        .await;
    // Expect NewMiningJob
    tester
        .check_next_v2(|msg: v2::messages::NewMiningJob| {
            test_utils::v2::message_check(msg, test_utils::v2::build_new_mining_job());
        })
        .await;
    // Expect SetNewPrevHash
    tester
        .check_next_v2(|msg: v2::messages::SetNewPrevHash| {
            test_utils::v2::message_check(msg, test_utils::v2::build_set_new_prev_hash());
        })
        .await;
    // Ensure that the V1 job has been registered
    let submit_template = V1SubmitTemplate {
        job_id: v1::messages::JobId::from_str(&test_utils::v1::MINING_NOTIFY_JOB_ID)
            .expect("BUG: cannot build JobId"),
        time: test_utils::common::MINING_WORK_NTIME,
        version: test_utils::common::MINING_WORK_VERSION,
    };

    let registered_submit_template = tester
        .translation
        .v2_to_v1_job_map
        .get(&0)
        .expect("BUG: No mining job with V2 ID 0");
    assert_eq!(
        submit_template,
        registered_submit_template.clone(),
        "New Mining Job ID not registered!"
    );
}

/// This test simulates incoming connection to the translation and verifies that the translation
/// emits corresponding V1 or V2 messages
/// TODO we need a way to detect that translation is not responding and the entire test should fail
#[tokio::test]
async fn test_setup_connection_translate() {
    let mut tester = TranslationTester::default();

    test_initial_sequence_translate(&mut tester).await;

    // Send SubmitShares
    tester.send_v2(test_utils::v2::build_submit_shares()).await;
    // Expect mining.submit to be generated
    let id = 3.into();
    tester
        .check_next_v1(id, |msg: v1::messages::Submit| {
            test_utils::v1::message_request_check(
                id,
                &msg,
                test_utils::v1::build_mining_submit(),
                test_utils::v1::MINING_SUBMIT_JSON,
            );
        })
        .await;

    // Simulate mining.submit response (true)
    tester
        .send_v1(test_utils::v1::build_mining_submit_ok_response_message())
        .await;
    // Expect SubmitSharesSuccess to be generated
    tester
        .check_next_v2(|msg: v2::messages::SubmitSharesSuccess| {
            test_utils::v2::message_check(msg, test_utils::v2::build_submit_shares_success());
        })
        .await;
}

#[tokio::test]
async fn test_shares_sequence_number_translate() {
    let mut tester = TranslationTester::default();

    test_initial_sequence_translate(&mut tester).await;

    let mut notify_v1 = test_utils::v1::build_mining_notify();
    let mut new_mining_job_v2 = test_utils::v2::build_new_mining_job();
    let mut new_prev_hash_v2 = test_utils::v2::build_set_new_prev_hash();

    let mut shares_v2 = test_utils::v2::build_submit_shares();
    let mut shares_success_v2 = test_utils::v2::build_submit_shares_success();
    let mut shares_error_v2 = test_utils::v2::build_submit_shares_error();
    let submit_v1 = test_utils::v1::build_mining_submit();

    const ERR_STALE: &str = "stale";

    // Send shares 1 for job 0
    tester.send_v2(shares_v2.clone()).await;
    tester
        .check_next_v1(3.into(), |msg: v1::messages::Submit| {
            assert_eq!(submit_v1, msg);
        })
        .await;

    tester
        .send_v1(test_utils::v1::build_ok_response_message(3))
        .await;
    tester
        .check_next_v2(|msg: v2::messages::SubmitSharesSuccess| {
            assert_eq!(shares_success_v2, msg);
        })
        .await;

    // Send shares 2 for job 0
    shares_v2.seq_num = 1;
    shares_v2.job_id = 0;

    tester.send_v2(shares_v2.clone()).await;
    tester
        .check_next_v1(4.into(), |msg: v1::messages::Submit| {
            assert_eq!(submit_v1, msg);
        })
        .await;

    notify_v1.clean_jobs = true;
    tester
        .send_v1(test_utils::v1::build_request_message(
            None,
            notify_v1.clone(),
        ))
        .await;

    // Send shares 3 for job 0
    shares_v2.seq_num = 2;
    shares_v2.job_id = 0;

    tester.send_v2(shares_v2.clone()).await;

    new_mining_job_v2.job_id = 1;
    new_prev_hash_v2.job_id = 1;
    tester
        .check_next_v2(|msg: v2::messages::NewMiningJob| {
            assert_eq!(new_mining_job_v2.clone(), msg);
        })
        .await;
    tester
        .check_next_v2(|msg: v2::messages::SetNewPrevHash| {
            assert_eq!(new_prev_hash_v2.clone(), msg);
        })
        .await;

    // Send shares 4 for job 1
    shares_v2.seq_num = 3;
    shares_v2.job_id = 1;

    tester.send_v2(shares_v2.clone()).await;

    tester
        .send_v1(test_utils::v1::build_err_response_message(4, 0, ERR_STALE))
        .await;

    shares_error_v2.seq_num = 1;
    shares_error_v2.code =
        v2::types::Str0_32::from_string(format!("ShareRjct:StratumError(0, \"{}", ERR_STALE));

    tester
        .check_next_v2(|msg: v2::messages::SubmitSharesError| {
            assert_eq!(shares_error_v2, msg);
        })
        .await;

    tester
        .send_v1(test_utils::v1::build_ok_response_message(5))
        .await;

    shares_error_v2.seq_num = 2;
    shares_error_v2.code = v2::types::Str0_32::try_from("General error: V2 Job ID not pre")
        .expect("BUG: cannot convert from str");
    tester
        .check_next_v2(|msg: v2::messages::SubmitSharesError| {
            assert_eq!(shares_error_v2, msg);
        })
        .await;

    shares_success_v2.last_seq_num = 3;
    tester
        .check_next_v2(|msg: v2::messages::SubmitSharesSuccess| {
            assert_eq!(shares_success_v2, msg);
        })
        .await;
}

#[test]
fn test_diff_1_bitcoin_target() {
    // Difficulty 1 target in big-endian format
    let difficulty_1_target_bytes: [u8; 32] = [
        0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    let expected_difficulty_1_target_uint256 = U256::from_big_endian(&difficulty_1_target_bytes);

    assert_eq!(
        expected_difficulty_1_target_uint256,
        V2ToV1Translation::DIFF1_TARGET,
        "Bitcoin difficulty 1 targets don't match exp: {:x?}, actual:{:x?}",
        expected_difficulty_1_target_uint256,
        V2ToV1Translation::DIFF1_TARGET
    );
}

#[test]
fn test_parse_client_reconnect() {
    use serde_json::Value;
    use v1::messages::ClientReconnect;

    assert_eq!(
        (
            Str0_255::try_from("").expect("BUG: cannot convert from str"),
            0
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![]))
            .expect(r#"BUG: Could not parse reconnect message without arguments"#)
    );

    // lower boundary case
    assert_eq!(
        (
            Str0_255::try_from("").expect("BUG: cannot convert from str"),
            0
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("".into()),
            Value::String("0".into()),
            Value::String("1".into()),
        ]))
        .expect(r#"BUG: Could not parse boundary_case with host="" and port="0"#)
    );

    // lower boundary case
    assert_eq!(
        (
            Str0_255::try_from("").expect("BUG: cannot convert from str"),
            0
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("".into()),
            Value::Number(0.into()),
            Value::Number(1.into()),
        ]))
        .expect(r#"BUG: Could not parse boundary_case with host="" and integeral port=0"#)
    );

    // random case
    assert_eq!(
        (
            Str0_255::try_from("some_host").expect("BUG: cannot convert from str"),
            1000
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("some_host".into()),
            Value::Number(1000.into()),
        ]))
        .expect(
            r#"BUG: Could not parse regular case with host="some_host" and integeral port=1000"#
        )
    );

    // upper boundary case
    assert_eq!(
        (
            Str0_255::from_string(repeat("h").take(255).collect::<String>()),
            65535
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String(repeat("h").take(255).collect::<String>()),
            Value::String("65535".into()),
            Value::String("1".into()),
        ]))
        .expect(
            r#"BUG: Could not parse boundary_case with longest valid host and string port="65535"."#
        )
    );

    // upper boundary cases
    assert_eq!(
        (
            Str0_255::from_string(repeat("h").take(255).collect::<String>()),
            65535
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String(repeat("h").take(255).collect::<String>()),
            Value::Number(65535.into()),
            Value::Number(1.into()),
        ]))
        .expect(
            r#"BUG: Could not parse boundary_case with longest valid host and integeral port=65535."#
        )
    );

    // non-ascii host name
    assert_eq!(
        (
            Str0_255::try_from("😊").expect("BUG: cannot convert from str"),
            1000
        ),
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("😊".into()),
            Value::Number(1000.into()),
        ]))
        .expect("BUG: Could not parse non-ascii utf-8 host-name string")
    );
}

/// Test port number overflow, hostname overflow, invalid port number string, hexadecimal string
#[test]
fn test_client_reconnect_parsing_with_invalid_arguments() {
    use v1::messages::ClientReconnect;

    if let Ok((_host, _port)) = V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
        Value::String("some_host".into()),
        Value::String("65536".into()), // invalid range
    ])) {
    } else if let Ok((_host, _port)) =
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("some_host".into()),
            Value::Number(65536.into()), // invalid range
        ]))
    {
        panic!("invalid port number integer not detected: {:?}", _port);
    } else if let Ok((_host, _port)) =
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String(repeat("h").take(256).collect::<String>()), // too long host name
            Value::Number(1000.into()),
        ]))
    {
        panic!("too long hostname not detected: {:?}", _host);
    } else if let Ok((_host, _port)) =
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("some_host".into()),
            Value::String("bad_non-numeric-port_description".into()), // invalid port string
        ]))
    {
        panic!("invalid non-numeric port value not detected: {:?}", _port);
    } else if let Ok((_host, _port)) =
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::String("some_host".into()),
            Value::Array(vec![1000.into()]), // invalid data type
        ]))
    {
        panic!("invalid port data type not detected")
    } else if let Ok((_host, _port)) =
        V2ToV1Translation::parse_client_reconnect(&ClientReconnect(vec![
            Value::Number(10.into()), // invalid data type
            Value::Number(1000.into()),
        ]))
    {
        panic!("invalid host name data type not detected")
    }
}
