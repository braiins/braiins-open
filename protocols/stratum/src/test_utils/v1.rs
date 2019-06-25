use serde::Serialize;
use slog::trace;
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::str::FromStr;

use super::common::*;
use crate::v1::framing::*;
use crate::v1::messages::*;
use crate::v1::{ExtraNonce1, HexBytes, V1Handler, V1Protocol};
use crate::LOGGER;

/// Testing subscribe request in a dense form without any spaces
pub const MINING_SUBSCRIBE_REQ_JSON: &str = concat!(
    r#"{"id":0,"method":"mining.subscribe","#,
    r#""params":["Braiins OS 2019-06-05",null,"stratum.slushpool.com",null]}"#
);

const EXTRA_NONCE_1: &str = "01650f001f25ea";
const EXTRA_NONCE_2_SIZE: usize = 4;

fn build_request_message<T>(id: Option<u32>, payload: T) -> Frame
where
    T: TryInto<RequestPayload> + std::fmt::Debug,
    <T as std::convert::TryInto<RequestPayload>>::Error: std::fmt::Debug,
{
    Frame::RpcRequest(Request {
        id,
        payload: payload.try_into().expect("Cannot serialize request"),
    })
}

pub fn build_subscribe_request_frame() -> Frame {
    build_request_message(Some(0), build_subscribe())
}

pub fn build_subscribe() -> Subscribe {
    Subscribe(
        Some(MINER_SW_SIGNATURE.into()), // agent_signature
        None,                            // extra_nonce1
        Some(POOL_URL.into()),           // url
        None,                            // port
    )
}

/// Random broken request
pub const MINING_BROKEN_REQ_JSON: &str = concat!(
    r#"{"id":1,"method":"mining.none_existing","#,
    r#""params":["10","12"]}"#
);

/// Subscribe success response in a dense form without any spaces
/// TODO: find out how to fill in extra nonce 1 and extra nonce 2 size from predefined constants
pub const MINING_SUBSCRIBE_OK_RESULT_JSON: &str = concat!(
    r#"{"id":0,"result":[[["mining.set_difficulty","1"],"#,
    r#"["mining.notify","1"]],"01650f001f25ea",4],"error":null}"#
);

fn build_result_response_message<T: Serialize>(id: u32, result: T) -> Frame {
    Frame::RpcResponse(Response {
        id,
        payload: ResponsePayload {
            result: Some(
                StratumResult::new_from(result).expect("Cannot build test response message"),
            ),
            error: None,
        },
    })
}

/// Special case for simple 'OK' response
fn build_ok_response_message(id: u32) -> Frame {
    build_result_response_message(id, BooleanResult(true))
}

pub fn build_subscribe_ok_response_frame() -> Frame {
    build_result_response_message(0, build_subscribe_ok_result())
}

pub fn build_authorize_ok_response_message() -> Frame {
    build_ok_response_message(1)
}

pub fn build_mining_submit_ok_response_message() -> Frame {
    build_ok_response_message(2)
}

pub fn build_subscribe_ok_result() -> SubscribeResult {
    SubscribeResult(
        vec![
            Subscription("mining.set_difficulty".to_string(), "1".to_string()),
            Subscription("mining.notify".to_string(), "1".to_string()),
        ],
        ExtraNonce1(HexBytes::try_from(EXTRA_NONCE_1).expect("Cannot parse extra nonce 1")),
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

pub fn build_stratum_err_response_frame() -> Response {
    Response {
        id: 1,
        payload: ResponsePayload {
            result: None,
            error: Some(build_stratum_error()),
        },
    }
}

pub const MINING_SET_DIFFICULTY_JSON: &str =
    r#"{"id":null,"method":"mining.set_difficulty","params":[512.0]}"#;

pub fn build_set_difficulty_request_message() -> Frame {
    build_request_message(None, build_set_difficulty())
}

pub fn build_set_difficulty() -> SetDifficulty {
    SetDifficulty([512f32])
}

pub const MINING_NOTIFY_JOB_ID: [u8; 1] = [0xbf; 1];

pub const MINING_NOTIFY_JSON: &str = concat!(
r#"{"#,
r#""id":null,"method":"mining.notify","#,
r#""params":["bf","4d16b6f85af6e2198f44ae2a6de67f78487ae5611b77c6c0440b921e00000000","#,
r#""01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff20020862062f503253482f04b8864e5008","#,
r#""072f736c7573682f000000000100f2052a010000001976a914d23fcdf86f7e756a64a7a9688ef9903327048ed988ac00000000","#,
r#"["4322158778c4f7f149b691134ab5eeed0437fc1754faa7b53deb32861b3e5a77","#,
r#""1680bad9bfb093f905bde05debe410a557e94a2bf8af3820c0dde29608009438","#,
r#""5a7f2aefb756fee2bcb94824ac98d71d3c3de2093dfba4e9288e35af786ab3d5","#,
r#""0d5facd3c63d1c14c97d732c3ebd6d7009f6f18f8142188de5baa78f1bc72b91"],"#,
r#""20000000","1725fd03","5d0ea025",false]"#,
r#"}"#,
);

/// This is merkle root for the job specified via MINING_NOTIFY_JSON
/// TODO: verify that this merkle root is correct assuming extra nonce 1 = 0, extra nonce 2 size = 4,
/// and extra nonce 2 =0
pub const MINING_NOTIFY_MERKLE_ROOT: &str =
    "085958174f0dceccf57a5e5c49641fbf821a0d2029b144fca97affeb7b561834";
// Original merkle root (doesn't match)
//    "a4eedd0736c8e5d316bbd77f683ce932e96f4cc8ac54159bdc8575903f0013f3";

pub fn build_mining_notify_request_message() -> Frame {
    build_request_message(None, build_mining_notify())
}

pub fn build_mining_notify() -> Notify {
    let deserialized = Frame::from_str(MINING_NOTIFY_JSON).expect("Cannot parse mining job");

    let notify = if let Frame::RpcRequest(req) = deserialized {
        Notify::try_from(req).expect("Cannot build mining notify message")
    } else {
        panic!("Wrong notification message");
    };

    notify
}

pub const MINING_SUBMIT_JSON: &str = concat!(
    r#"{"id":1,"method":"mining.submit","params":["braiins.worker0","#,
    r#""bf","00000000","5d0ea025","efbeadde","20000000"]}"#
);

pub fn build_mining_submit_request_message() -> Frame {
    build_request_message(None, build_mining_submit())
}

pub fn build_mining_submit() -> Submit {
    let deserialized = Frame::from_str(MINING_SUBMIT_JSON).expect("Cannot parse mining job");

    let submit = if let Frame::RpcRequest(req) = deserialized {
        Submit::try_from(req).expect("Cannot build mining submit message")
    } else {
        panic!("Wrong notification message");
    };

    submit
}

pub const MINING_AUTHORIZE_JSON: &str =
    r#"{"id":1,"method":"mining.authorize","params":["braiins.worker0",""]}"#;

pub fn build_authorize_request_message() -> Frame {
    build_request_message(Some(1), build_authorize())
}

pub fn build_authorize() -> Authorize {
    Authorize(USER_CREDENTIALS.to_string(), "".to_string())
}

pub const MINING_AUTHORIZE_OK: &str = r#"{"id": 1,"error":null,"result":true}"#;

/// Message payload visitor that compares the payload of the visited message (e.g. after
/// deserialization test) with the payload built.
/// This handler should be used in tests to verify that serialization and deserialization yield the
/// same results
pub struct TestIdentityHandler;
//pub struct TestIdentityHandler(fn()->Strat);

impl TestIdentityHandler {
    /// Performs 2 checks:
    /// - if the provided message payload matches the one expected by the test (provided by
    /// `build_payload` function
    /// - whether the `full_message` after serialization matches the expected `json_message` JSON
    /// representation
    fn visit_and_check<P, F>(
        &mut self,
        msg: &wire::Message<V1Protocol>,
        payload: &P,
        build_payload: F,
        full_message: Frame,
        json_message: &str,
    ) where
        P: Debug + PartialEq,
        F: FnOnce() -> P,
    {
        // Build expected payload for verifying correct deserialization
        let expected_payload = build_payload();
        trace!(
            LOGGER,
            "V1 TestIdentityHandler: Message ID {:?} {:?}",
            msg.id,
            payload
        );
        assert_eq!(expected_payload, *payload, "Message payloads don't match");

        let serialized_message: wire::TxFrame = full_message.try_into().expect("Cannot serialize");
        assert_eq!(
            json_message,
            std::str::from_utf8(&serialized_message)
                .expect("Can't convert serialized message to str"),
            "Serialized messages don't match"
        );
    }

    fn visit_and_check_request<P, F>(
        &mut self,
        msg: &wire::Message<V1Protocol>,
        payload: &P,
        build_payload: F,
        json_message: &str,
    ) where
        P: Debug + PartialEq + Clone + TryInto<RequestPayload>,
        <P as std::convert::TryInto<RequestPayload>>::Error: std::fmt::Debug,
        F: FnOnce() -> P,
    {
        self.visit_and_check(
            msg,
            payload,
            build_payload,
            build_request_message(msg.id, payload.clone()),
            json_message,
        );
    }
}

impl V1Handler for TestIdentityHandler {
    fn visit_stratum_result(&mut self, msg: &wire::Message<V1Protocol>, payload: &StratumResult) {
        let full_message =
            build_result_response_message(msg.id.expect("Message ID missing"), payload);

        self.visit_and_check(
            msg,
            payload,
            || {
                StratumResult::new_from(build_subscribe_ok_result())
                    .expect("Cannot convert to stratum result")
            },
            build_result_response_message(msg.id.expect("Message ID missing"), payload),
            MINING_SUBSCRIBE_OK_RESULT_JSON,
        );
    }

    fn visit_subscribe(&mut self, msg: &wire::Message<V1Protocol>, payload: &Subscribe) {
        // we have to clone the payload to create a locally owned copy as build_request_message
        // requires transfer of ownership
        self.visit_and_check_request(msg, payload, build_subscribe, MINING_SUBSCRIBE_REQ_JSON);
    }

    fn visit_authorize(&mut self, msg: &wire::Message<V1Protocol>, payload: &Authorize) {
        self.visit_and_check_request(msg, payload, build_authorize, MINING_AUTHORIZE_JSON);
    }

    fn visit_set_difficulty(&mut self, msg: &wire::Message<V1Protocol>, payload: &SetDifficulty) {
        self.visit_and_check_request(
            msg,
            payload,
            build_set_difficulty,
            MINING_SET_DIFFICULTY_JSON,
        );
    }

    fn visit_notify(&mut self, msg: &wire::Message<V1Protocol>, payload: &Notify) {
        self.visit_and_check_request(msg, payload, build_mining_notify, MINING_NOTIFY_JSON);
    }

    fn visit_submit(&mut self, msg: &wire::Message<V1Protocol>, payload: &Submit) {
        self.visit_and_check_request(msg, payload, build_mining_submit, MINING_SUBMIT_JSON);
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
