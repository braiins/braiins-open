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

use async_trait::async_trait;
use bytes::BytesMut;
use serde::Serialize;
use std::collections::VecDeque;
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::str::FromStr;

use ii_logging::macros::*;
use ii_unvariant::handler;

use super::common::*;
use crate::error::Result;
use crate::v1::{framing::*, messages::*, rpc::*, ExtraNonce1, HexBytes, MessageId};

pub enum TestMessage {
    MsgSubscribe(MessageId, Subscribe),
    MsgExtranonceSubscribe(MessageId, ExtranonceSubscribe),
    MsgAuthorize(MessageId, Authorize),
    MsgSetDifficulty(MessageId, SetDifficulty),
    MsgSetExtranonce(MessageId, SetExtranonce),
    MsgConfigure(MessageId, Configure),
    MsgSubmit(MessageId, Submit),
    MsgNotify(MessageId, Notify),
    MsgSetVersionMask(MessageId, SetVersionMask),
    MsgClientReconnect(MessageId, ClientReconnect),
    MsgStratumResult(MessageId, StratumResult),
}

macro_rules! impl_unwrap {
    ($method:ident, $from_enum:ident, $to_msg:ident) => {
        pub fn $method(self, expected_id: MessageId) -> $to_msg {
            match self {
                Self::$from_enum(id, msg) => {
                    assert_eq!(id, expected_id, "BUG: unexpected id");
                    msg
                }
                _ => panic!("BUG: expected '{}'", stringify!($to_msg)),
            }
        }
    };
}

macro_rules! impl_unwrap_result {
    ($method:ident, $to_msg:ident) => {
        pub fn $method(self, expected_id: MessageId) -> $to_msg {
            match self {
                Self::MsgStratumResult(id, msg) => {
                    assert_eq!(id, expected_id, "BUG: unexpected id");
                    serde_json::from_value(msg.0)
                        .expect(format!("BUG: cannot serialize '{}'", stringify!($to_msg)).as_str())
                }
                _ => panic!("BUG: expected '{}'", stringify!($to_msg)),
            }
        }
    };
}

impl TestMessage {
    impl_unwrap!(unwrap_subscribe, MsgSubscribe, Subscribe);
    impl_unwrap!(
        unwrap_extranonce_subscribe,
        MsgExtranonceSubscribe,
        ExtranonceSubscribe
    );
    impl_unwrap!(unwrap_authorize, MsgAuthorize, Authorize);
    impl_unwrap!(unwrap_set_difficulty, MsgSetDifficulty, SetDifficulty);
    impl_unwrap!(unwrap_set_extranonce, MsgSetExtranonce, SetExtranonce);
    impl_unwrap!(unwrap_configure, MsgConfigure, Configure);
    impl_unwrap!(unwrap_submit, MsgSubmit, Submit);
    impl_unwrap!(unwrap_notify, MsgNotify, Notify);
    impl_unwrap!(unwrap_set_version_mask, MsgSetVersionMask, SetVersionMask);
    impl_unwrap!(unwrap_client_reconnect, MsgClientReconnect, ClientReconnect);

    impl_unwrap_result!(unwrap_subscribe_result, SubscribeResult);
    impl_unwrap_result!(unwrap_configure_result, ConfigureResult);
    impl_unwrap_result!(unwrap_boolean_result, BooleanResult);
}

macro_rules! impl_from_msg_to_enum {
    ($from_msg:ident, $to_enum:ident) => {
        impl From<(MessageId, $from_msg)> for TestMessage {
            fn from(id_msg: (MessageId, $from_msg)) -> Self {
                let (id, msg) = id_msg;
                Self::$to_enum(id, msg)
            }
        }
    };
}

macro_rules! impl_try_from_enum_to_msg {
    ($from_enum:ident, $to_msg:ident) => {
        impl TryFrom<(MessageId, TestMessage)> for $to_msg {
            type Error = ();

            fn try_from(
                id_msg: (MessageId, TestMessage),
            ) -> std::result::Result<Self, Self::Error> {
                let (expected_id, msg) = id_msg;
                match msg {
                    TestMessage::$from_enum(id, msg) if id == expected_id => Ok(msg),
                    _ => Err(()),
                }
            }
        }
    };
}

macro_rules! impl_try_from_result_to_msg {
    ($to_msg:ident) => {
        impl TryFrom<(MessageId, TestMessage)> for $to_msg {
            type Error = ();

            fn try_from(
                id_msg: (MessageId, TestMessage),
            ) -> std::result::Result<Self, Self::Error> {
                let (expected_id, msg) = id_msg;
                match msg {
                    TestMessage::MsgStratumResult(id, msg) if id == expected_id => {
                        Ok(serde_json::from_value(msg.0).expect(
                            format!("BUG: cannot serialize '{}'", stringify!($to_msg)).as_str(),
                        ))
                    }
                    _ => Err(()),
                }
            }
        }
    };
}

macro_rules! impl_conversions {
    ($msg:ident, $test_enum:ident) => {
        impl_from_msg_to_enum!($msg, $test_enum);
        impl_try_from_enum_to_msg!($test_enum, $msg);
    };
}

impl_conversions!(Subscribe, MsgSubscribe);
impl_conversions!(ExtranonceSubscribe, MsgExtranonceSubscribe);
impl_conversions!(Authorize, MsgAuthorize);
impl_conversions!(SetDifficulty, MsgSetDifficulty);
impl_conversions!(SetExtranonce, MsgSetExtranonce);
impl_conversions!(Configure, MsgConfigure);
impl_conversions!(Submit, MsgSubmit);
impl_conversions!(Notify, MsgNotify);
impl_conversions!(SetVersionMask, MsgSetVersionMask);
impl_conversions!(ClientReconnect, MsgClientReconnect);

impl_from_msg_to_enum!(StratumResult, MsgStratumResult);
impl_try_from_result_to_msg!(SubscribeResult);
impl_try_from_result_to_msg!(ConfigureResult);
impl_try_from_result_to_msg!(BooleanResult);

#[derive(Default)]
pub struct TestCollectorHandler {
    messages: VecDeque<TestMessage>,
}

#[handler(async try Rpc suffix _v1)]
impl TestCollectorHandler {
    async fn handle_subscribe(&mut self, id_msg: (MessageId, Subscribe)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_extranonce_subscribe(&mut self, id_msg: (MessageId, ExtranonceSubscribe)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_authorize(&mut self, id_msg: (MessageId, Authorize)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_set_difficulty(&mut self, id_msg: (MessageId, SetDifficulty)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_set_extranonce(&mut self, id_msg: (MessageId, SetExtranonce)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_configure(&mut self, id_msg: (MessageId, Configure)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_submit(&mut self, id_msg: (MessageId, Submit)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_notify(&mut self, id_msg: (MessageId, Notify)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_setversion_mask(&mut self, id_msg: (MessageId, SetVersionMask)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_client_reconnect(&mut self, id_msg: (MessageId, ClientReconnect)) {
        self.messages.push_back(id_msg.into());
    }

    async fn handle_stratum_result(&mut self, id_msg: (MessageId, StratumResult)) {
        self.messages.push_back(id_msg.into());
    }

    #[handle(_)]
    async fn handle_everything(&mut self, rpc: Result<Rpc>) {
        panic!("BUG: No handler method for received rpc: {:?}", rpc);
    }
}

impl Iterator for TestCollectorHandler {
    type Item = TestMessage;

    fn next(&mut self) -> Option<Self::Item> {
        self.messages.pop_front()
    }
}

#[async_trait]
pub trait TestFrameReceiver {
    async fn receive_v1(&mut self) -> Rpc;

    async fn next_v1(&mut self) -> TestMessage {
        let rpc = self.receive_v1().await;
        let mut handler = TestCollectorHandler::default();
        handler.handle_v1(rpc).await;
        handler.next().expect("BUG: No message was received")
    }

    /// Convert received `TestMessage` to expected structure (that can be passed to closure `f`
    /// for further processing)
    async fn check_next_v1<T, U, V>(&mut self, expected_id: MessageId, f: T) -> V
    where
        T: FnOnce(U) -> V + Send + Sync,
        U: TryFrom<(MessageId, TestMessage), Error = ()>,
    {
        let msg = self.next_v1().await;
        f(U::try_from((expected_id, msg))
            .expect(format!("BUG: expected '{}'", stringify!(U)).as_str()))
    }
}

pub const MINING_CONFIGURE_REQ_JSON: &str = concat!(
    r#"{"id":0,"method":"mining.configure","#,
    r#""params":[["version-rolling"],"#,
    r#"{"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":16}]}"#
);

pub fn build_configure_request() -> Rpc {
    build_request_message(Some(0), build_configure())
}

pub fn build_configure() -> Configure {
    let v = VersionRolling::new(
        crate::BIP320_N_VERSION_MASK,
        crate::BIP320_N_VERSION_MAX_BITS,
    );

    let mut configure = Configure::new();
    configure
        .add_feature(v)
        .expect("BUG: Could not add Configure feature");

    configure
}

pub const MINING_CONFIGURE_OK_RESP_JSON: &str = concat!(
    r#"{"id":0,"error":null,"result": {"version-rolling":true,"#,
    r#""version-rolling.mask":"1fffe000"}}"#
);

pub fn build_configure_ok_response_message() -> Rpc {
    let cfg: ConfigureResult =
        serde_json::from_str(r#"{"version-rolling":true,"version-rolling.mask":"1fffe000"}"#)
            .expect("BUG: configure_ok_response deserialization failed");
    trace!("build_configure_ok_response_message() {:?}", cfg);
    build_result_response_message(0, cfg)
}

/// Testing subscribe request in a dense form without any spaces
pub const MINING_SUBSCRIBE_REQ_JSON: &str = concat!(
    r#"{"id":1,"method":"mining.subscribe","#,
    r#""params":["Braiins OS 2019-06-05",null,"stratum.slushpool.com:3333",null]}"#
);

const EXTRA_NONCE_1: &str = "6c6f010000000c";
const EXTRA_NONCE_2_SIZE: usize = 4;

pub fn build_request_message<T>(id: MessageId, payload: T) -> Rpc
where
    T: TryInto<RequestPayload> + Debug,
    <T as std::convert::TryInto<RequestPayload>>::Error: std::fmt::Debug,
{
    Rpc::from(Request {
        id,
        payload: payload.try_into().expect("BUG: Cannot serialize request"),
    })
}

pub fn build_subscribe_request_frame() -> Rpc {
    build_request_message(Some(1), build_subscribe())
}

pub fn build_subscribe() -> Subscribe {
    let hostname_port = format!("{}:{}", String::from(POOL_URL), POOL_PORT);
    Subscribe {
        agent_signature: Some(MINER_SW_SIGNATURE.into()),
        extra_nonce1: None,
        url: Some(hostname_port),
        port: None,
    }
}

/// String contains non-empty both fields: "result" and "error", which is not permitted
/// in JSON-RPC specification
pub const CORRECTABLE_BROKEN_RESPONSE_JSON: &str = concat!(
    r#"{"id": 33, "result": false, "error": "#,
    r#"[21, "Job not found (=stale)", null]}"#
);

/// String contains unrecognized field and should not succeed to parse
pub const FULLY_BROKEN_RESPONSE_JSON: &str = concat!(
    r#"{"id": 33, "custom_result": 13, "error": "#,
    r#"[21, "Job not found (=stale)", null]}"#
);

/// Random broken request
pub const MINING_BROKEN_REQ_JSON: &str = concat!(
    r#"{"id":1,"method":"mining.none_existing","#,
    r#""params":["10","12"]}"#
);

/// Subscribe success response in a dense form without any spaces
/// TODO: find out how to fill in extra nonce 1 and extra nonce 2 size from predefined constants
pub const MINING_SUBSCRIBE_OK_RESULT_JSON: &str = concat!(
    r#"{"id":1,"#,
    r#""result":[[],"6c6f010000000c",4],"#,
    r#""error":null}"#
);

fn build_result_response_message<T: Serialize>(id: u32, result: T) -> Rpc {
    Rpc::from(Response {
        id,
        stratum_result: Some(
            StratumResult::new(result).expect("BUG: Cannot build test response message"),
        ),
        stratum_error: None,
    })
}

/// Special case for simple 'OK' response
pub fn build_ok_response_message(id: u32) -> Rpc {
    build_result_response_message(id, BooleanResult(true))
}

pub fn build_err_response_message(id: u32, code: i32, msg: &str) -> Rpc {
    Rpc::from(Response {
        id,
        stratum_result: None,
        stratum_error: Some(StratumError(code, msg.to_string(), None)),
    })
}

pub fn build_subscribe_ok_response_message() -> Rpc {
    build_result_response_message(1, build_subscribe_ok_result())
}

pub fn build_authorize_ok_response_message() -> Rpc {
    build_ok_response_message(2)
}

pub fn build_mining_submit_ok_response_message() -> Rpc {
    build_ok_response_message(3)
}

pub fn build_subscribe_ok_result() -> SubscribeResult {
    SubscribeResult(
        vec![],
        ExtraNonce1(HexBytes::try_from(EXTRA_NONCE_1).expect("BUG: Cannot parse extra nonce 1")),
        EXTRA_NONCE_2_SIZE,
    )
}

/// Subscribe success response in a dense form without any spaces
/// TODO: find out how to fill in extra nonce 1 and extra nonce 2 size from predefined constants
pub const MINING_SUBSCRIBE_MALFORMED_RESULT_JSON: &str =
    r#"{"id":1,"result":["01650f001f25ea",4],"error":null}"#;

/// Testing error response in a dense form without any spaces
pub const STRATUM_ERROR_JSON: &str = r#"{"id":1,"result":null,"error":[20,"Other/Unknown",null]}"#;

pub fn build_stratum_error() -> StratumError {
    StratumError(20, "Other/Unknown".into(), None)
}

pub fn build_stratum_err_response() -> Rpc {
    Rpc::from(Response {
        id: 1,
        stratum_result: None,
        stratum_error: Some(build_stratum_error()),
    })
}

pub const MINING_SET_DIFFICULTY_JSON: &str =
    r#"{"id":null,"method":"mining.set_difficulty","params":[4.0]}"#;

pub fn build_set_difficulty_request_message() -> Rpc {
    build_request_message(None, build_set_difficulty())
}

pub fn build_set_difficulty() -> SetDifficulty {
    SetDifficulty::from(4f32)
}

pub const MINING_NOTIFY_JOB_ID: &str = "ahoj";
pub const MINING_NOTIFY_JSON: &str = concat!(
    r#"{"#,
    r#""id":null,"method":"mining.notify","#,
    r#""params":["ahoj","13f46cc7bf03a16697170dbb9d15680b7e75fcf10846037f171d7f6b00000000","01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff44026d0cfabe6d6dc22da09055dabfce93b90fec9c53cbec5ace52248db605efe1d2f2c1bfc8f1260100000000000000","e91d012f736c7573682f000000000200f2052a010000001976a914505b9f58045298b98a7af6333445098ac700ac3088ac0000000000000000266a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf900000000",[],"20000000","1d00ffff","5d10bc0a",false]"#,
    r#"}"#,
);

/// This is merkle root for the job specified via MINING_NOTIFY_JSON
/// TODO: verify that this merkle root is correct assuming extra nonce 1 = 0, extra nonce 2 size = 4,
/// and extra nonce 2 =0
pub const MINING_NOTIFY_MERKLE_ROOT: &str =
    "91176a137779ca8a591fa94461210fc2d62e607c7aef93ed38dff510f0d946a2";
//"085958174f0dceccf57a5e5c49641fbf821a0d2029b144fca97affeb7b561834";
// Original merkle root (doesn't match)
//    "a4eedd0736c8e5d316bbd77f683ce932e96f4cc8ac54159bdc8575903f0013f3";

pub fn build_mining_notify_request_message() -> Rpc {
    build_request_message(None, build_mining_notify())
}

pub fn build_mining_notify() -> Notify {
    let deserialized = Rpc::from_str(MINING_NOTIFY_JSON).expect("BUG: Cannot parse mining job");

    let notify = if let Rpc::Request(req) = deserialized {
        Notify::try_from(req).expect("BUG: Cannot build mining notify message")
    } else {
        panic!("Wrong notification message");
    };

    notify
}

pub const MINING_SUBMIT_JSON: &str = concat!(
    r#"{"id":3,"method":"mining.submit","#,
    // TODO the correct share extra nonce 2 is 01000000, we have to replace the sample job
    // completely with a new none that has extra nonce 2 == 0
    r#""params":["braiins.worker0","ahoj","00000000","5d10bc0a","0443c37b","00000000"]"#,
    r#"}"#
);

pub fn build_mining_submit_request_message() -> Rpc {
    build_request_message(None, build_mining_submit())
}

pub fn build_mining_submit() -> Submit {
    let deserialized = Rpc::from_str(MINING_SUBMIT_JSON).expect("BUG: Cannot parse mining job");

    let submit = if let Rpc::Request(req) = deserialized {
        Submit::try_from(req).expect("BUG: Cannot build mining submit message")
    } else {
        panic!("Wrong notification message");
    };

    submit
}

pub const MINING_AUTHORIZE_JSON: &str =
    r#"{"id":2,"method":"mining.authorize","params":["braiins.worker0",""]}"#;

pub fn build_authorize_request_message() -> Rpc {
    build_request_message(Some(1), build_authorize())
}

pub fn build_authorize() -> Authorize {
    Authorize {
        name: USER_CREDENTIALS.to_string(),
        password: "".to_string(),
    }
}

pub const MINING_AUTHORIZE_OK: &str = r#"{"id": 1,"error":null,"result":true}"#;

pub const CLIENT_RECONNECT_JSON: &str =
    r#"{"id":1,"method":"client.reconnect","params":["stratum.slushpool.com", 3333, 1]}"#;

pub fn build_client_reconnect_request_message() -> Rpc {
    build_request_message(Some(1), build_client_reconnect())
}

pub fn build_client_reconnect() -> ClientReconnect {
    let deserialized =
        Rpc::from_str(CLIENT_RECONNECT_JSON).expect("BUG: Cannot parse reconnect message");
    let reconnect = if let Rpc::Request(req) = deserialized {
        ClientReconnect::try_from(req).expect("BUG: Cannot build reconnect message")
    } else {
        panic!("Wrong reconnect message");
    };
    reconnect
}

/// Performs 2 checks:
/// - if the provided message payload matches the one expected by the test (`expected_payload`)
/// - whether the `full_message` after serialization matches the expected `json_message` JSON
/// representation
pub fn message_check<P>(
    id: MessageId,
    payload: &P,
    expected_payload: P,
    full_message: Rpc,
    json_message: &str,
) where
    P: Debug + PartialEq,
{
    trace!("V1: Message ID {:?} {:?}", id, payload);
    assert_eq!(expected_payload, *payload, "Message payloads don't match");

    // Build frame from the provided Rpc message and use its serialization for test evaluation
    let message_frame: Frame = full_message
        .try_into()
        .expect("BUG: Cannot build frame from Rpc");
    let mut serialized_frame = BytesMut::new();
    message_frame
        .serialize(&mut serialized_frame)
        .expect("BUG: Cannot serialize frame");
    assert_eq!(
        json_message,
        std::str::from_utf8(&serialized_frame[..])
            .expect("BUG: Can't convert serialized message to str"),
        "Serialized messages don't match"
    );
}

pub fn message_request_check<P>(id: MessageId, payload: &P, expected_payload: P, json_message: &str)
where
    P: Debug + PartialEq + Clone + TryInto<RequestPayload>,
    <P as std::convert::TryInto<RequestPayload>>::Error: std::fmt::Debug,
{
    message_check(
        id,
        payload,
        expected_payload,
        build_request_message(id, payload.clone()),
        json_message,
    );
}

/// Message payload visitor that compares the payload of the visited message (e.g. after
/// deserialization test) with the payload built.
/// This handler should be used in tests to verify that serialization and deserialization yield the
/// same results
pub struct TestIdentityHandler;

#[handler(async try Rpc suffix _v1)]
impl TestIdentityHandler {
    async fn handle_stratum_result(&mut self, res: (MessageId, StratumResult)) {
        let (id, res) = res;
        message_check(
            id,
            &res,
            StratumResult::new(build_subscribe_ok_result())
                .expect("BUG: Cannot convert to stratum result"),
            build_result_response_message(id.expect("BUG: Message ID missing"), &res),
            MINING_SUBSCRIBE_OK_RESULT_JSON,
        );
    }

    async fn handle_notify(&mut self, id_msg: (MessageId, Notify)) {
        let (id, msg) = id_msg;
        message_request_check(id, &msg, build_mining_notify(), MINING_NOTIFY_JSON);
    }

    async fn handle_configure(&mut self, id_msg: (MessageId, Configure)) {
        let (id, msg) = id_msg;
        message_request_check(id, &msg, build_configure(), MINING_CONFIGURE_REQ_JSON);
    }

    async fn handle_subscribe(&mut self, id_msg: (MessageId, Subscribe)) {
        let (id, msg) = id_msg;
        message_request_check(id, &msg, build_subscribe(), MINING_SUBSCRIBE_REQ_JSON);
    }

    async fn handle_authorize(&mut self, id_msg: (MessageId, Authorize)) {
        let (id, msg) = id_msg;
        message_request_check(id, &msg, build_authorize(), MINING_AUTHORIZE_JSON);
    }

    async fn handle_set_difficulty(&mut self, id_msg: (MessageId, SetDifficulty)) {
        let (id, msg) = id_msg;
        message_request_check(id, &msg, build_set_difficulty(), MINING_SET_DIFFICULTY_JSON);
    }

    async fn handle_submit(&mut self, id_msg: (MessageId, Submit)) {
        let (id, msg) = id_msg;
        message_request_check(id, &msg, build_mining_submit(), MINING_SUBMIT_JSON);
    }

    #[handle(_)]
    async fn handle_rest(&mut self, rpc: Result<Rpc>) {
        panic!("Unexpected v1 message: {:?}", rpc);
    }
}

/// A complete list of all requests in this module for massive testing
/// TODO Is it possible to use inventory crate to collect these?
pub const V1_TEST_REQUESTS: &[&str] = &[
    MINING_SUBSCRIBE_REQ_JSON,
    MINING_NOTIFY_JSON,
    MINING_AUTHORIZE_JSON,
    MINING_SUBSCRIBE_REQ_JSON,
    MINING_SET_DIFFICULTY_JSON,
    MINING_SUBMIT_JSON,
];
