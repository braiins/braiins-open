use std::str::FromStr;

use super::*;
use crate::test_utils::v1::*;
use crate::v1::framing;

#[test]
fn test_build_subscribe_from_rpc_request() {
    let subscribe_req = build_subscribe_rpc_request();
    let expected_subscribe = build_subscribe();
    let subscribe = Subscribe::try_from(subscribe_req).expect("Conversion failed");

    assert_eq!(expected_subscribe, subscribe, "Subscribe request mismatch");
}

#[test]
fn test_build_subscribe_good_result_from_rpc_response() {
    let subscribe_resp = build_subscribe_ok_rpc_response();
    let expected_subscribe_result = build_subscribe_ok_result();
    let subscribe_result = SubscribeResult::try_from(subscribe_resp).expect("Conversion failed");

    assert_eq!(
        expected_subscribe_result, subscribe_result,
        "Subscribe result mismatch"
    );
}

#[test]
fn test_build_subscribe_good_result_json() {
    let expected_subscribe_result = build_subscribe_ok_result();
    match framing::Frame::from_str(MINING_SUBSCRIBE_OK_RESULT_JSON)
        .expect("Cannot prepare test result")
    {
        framing::Frame::RpcResponse(resp) => {
            let subscribe_result = SubscribeResult::try_from(resp).expect("Conversion failed");
            assert_eq!(
                expected_subscribe_result, subscribe_result,
                "Subscribe result mismatch"
            );
        }
        framing::Frame::RpcRequest(req) => {
            assert!(false, "Received request ({:?} instead of response", req);
        }
    }
}

#[test]
#[should_panic]
fn test_subscribe_malformed_result_json() {
    let expected_subscribe_result = build_subscribe_ok_result();
    match framing::Frame::from_str(MINING_SUBSCRIBE_MALFORMED_RESULT_JSON)
        .expect("Cannot prepare test result")
    {
        framing::Frame::RpcResponse(resp) => {
            let subscribe_result = SubscribeResult::try_from(resp).expect("Conversion failed");
            assert_eq!(
                expected_subscribe_result, subscribe_result,
                "Subscribe result mismatch"
            );
        }
        framing::Frame::RpcRequest(req) => {
            assert!(false, "Received request ({:?} instead of response", req);
        }
    }
}
