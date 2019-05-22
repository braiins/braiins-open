use crate::v1::framing::*;
use crate::v1::messages::*;
use crate::v1::{ExtraNonce1, V1Handler, V1Protocol};
use serde_json::Value;
use std::convert::TryFrom;
use std::fmt::Debug;

/// Testing subscribe request in a dense form without any spaces
pub const MINING_SUBSCRIBE_REQ_JSON: &str = concat!(
    r#"{"id":1,"method":"mining.subscribe","#,
    r#""params":["Braiins OS",null,"stratum.slushpool.com","3333"]}"#
);

const MINER_SW_SIGNATURE: &str = "Braiins OS";
const POOL_URL: &str = "stratum.slushpool.com";
const POOL_PORT: usize = 3333;
const EXTRA_NONCE_1: &str = "01650f001f25ea";
const EXTRA_NONCE_2_SIZE: usize = 4;

pub fn build_subscribe_rpc_request() -> Request {
    Request {
        id: Some(1),
        // TODO reuse build_subscribe() + try_from
        payload: RequestPayload {
            method: Method::Subscribe,
            params: vec![
                MINER_SW_SIGNATURE.into(),
                Value::Null,
                POOL_URL.into(),
                format!("{}", POOL_PORT).into(),
            ],
        },
    }
}

pub fn build_subscribe() -> Subscribe {
    Subscribe {
        agent_signature: Some(MINER_SW_SIGNATURE.into()),
        extra_nonce1: None,
        url: Some(POOL_URL.into()),
        port: Some(format!("{}", POOL_PORT)),
    }
}

/// Random broken request
pub const MINING_BROKEN_REQ_JSON: &str = concat!(
    r#"{"id":1,"method":"mining.none_existing","#,
    r#""params":["10","12"]}"#
);

/// Subscribe success response in a dense form without any spaces
/// TODO: find out how to fill in extra nonce 1 and extra nonce 2 size from predefined constants
pub const MINING_SUBSCRIBE_OK_RESULT_JSON: &str = concat!(
    r#"{"id":1,"result":[[["mining.set_difficulty","1"],"#,
    r#"["mining.notify","1"]],"01650f001f25ea",4],"error":null}"#
);

pub fn build_subscribe_ok_rpc_response() -> Response {
    Response {
        id: 1,
        payload: ResponsePayload {
            result: Some(
                StratumResult::new_from(build_subscribe_ok_result())
                    .expect("Cannot build test subscribe response"),
            ),
            error: None,
        },
    }
}

pub fn build_subscribe_ok_result() -> SubscribeResult {
    SubscribeResult(
        vec![
            Subscription("mining.set_difficulty".to_string(), "1".to_string()),
            Subscription("mining.notify".to_string(), "1".to_string()),
        ],
        ExtraNonce1::try_from(EXTRA_NONCE_1).expect("Cannot parse extra nonce 1"),
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

pub fn build_stratum_err_rpc_response() -> Response {
    Response {
        id: 1,
        payload: ResponsePayload {
            result: None,
            error: Some(build_stratum_error()),
        },
    }
}

/// Message payload visitor that compares the payload of the visited message (e.g. after
/// deserialization test) with the payload built.
/// This handler should be used in tests to verify that serialization and deserialization yield the
/// same results
pub struct TestIdentityHandler;

impl TestIdentityHandler {
    fn visit_and_check<P, F>(&self, msg: &wire::Message<V1Protocol>, payload: &P, build: F)
    where
        P: Debug + PartialEq,
        F: FnOnce() -> P,
    {
        // Build expected payload for verifying correct deserialization
        let expected_payload = build();
        println!("XXXXMessage ID {:?} {:x?}", msg.id, payload);
        assert_eq!(expected_payload, *payload, "Message payloads don't match");
    }
}

impl V1Handler for TestIdentityHandler {
    fn visit_subscribe(&self, msg: &wire::Message<V1Protocol>, payload: &Subscribe) {
        self.visit_and_check(msg, payload, build_subscribe);
    }

    fn visit_stratum_result(&self, msg: &wire::Message<V1Protocol>, payload: &StratumResult) {
        self.visit_and_check(msg, payload, || {
            StratumResult::new_from(build_subscribe_ok_result())
                .expect("Cannot convert to stratum result")
        });
    }
}
