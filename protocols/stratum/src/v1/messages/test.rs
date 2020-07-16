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

use std::str::FromStr;

use super::*;
use crate::test_utils::v1::*;
use crate::v1::rpc::Rpc;
use serde_json;
use serde_json::Value;

#[test]
fn set_version_mask_serialize() {
    let msg = SetVersionMask {
        mask: VersionMask(HexU32Be(0xdeadbeef)),
    };

    let msg_json = serde_json::to_string(&msg).expect("BUG: Failed to serialize SetVersionMask");
    assert_eq!(msg_json, "[\"deadbeef\"]");
    let msg_des: SetVersionMask =
        serde_json::from_str(&msg_json).expect("BUG: Failed to deserialize SetVersionMask");
    assert_eq!(msg_des, msg);

    let payload = msg.try_into().expect("BUG: failed to build RPC payload");
    let lhs = rpc::Request {
        id: Some(1),
        payload,
    };
    let lhs = serde_json::to_value(lhs).expect("BUG: failed to serialize RPC payload");

    let rhs = r#"{"id": 1, "method": "mining.set_version_mask", "params": ["deadbeef"]}"#;
    let rhs: Value =
        serde_json::from_str(&rhs).expect("BUG: failed to deserialize static JSON string");
    assert_eq!(lhs, rhs);
}

#[test]
fn test_build_subscribe_from_rpc_request() {
    if let Rpc::Request(subscribe_req) = build_subscribe_request_frame() {
        let expected_subscribe = build_subscribe();
        let subscribe = Subscribe::try_from(subscribe_req).expect("BUG: Conversion failed");

        assert_eq!(expected_subscribe, subscribe, "Subscribe request mismatch");
    } else {
        assert!(false, "Request expected");
    }
}

#[test]
fn test_build_subscribe_good_result_from_response() {
    if let Rpc::Response(subscribe_resp) = build_subscribe_ok_response_message() {
        let expected_subscribe_result = build_subscribe_ok_result();
        let subscribe_result =
            SubscribeResult::try_from(subscribe_resp).expect("BUG: Conversion failed");

        assert_eq!(
            expected_subscribe_result, subscribe_result,
            "Subscribe result mismatch"
        );
    } else {
        assert!(false, "Response expected, the test needs to be fixed")
    }
}

#[test]
fn test_build_subscribe_good_result_json() {
    let expected_subscribe_result = build_subscribe_ok_result();
    match Rpc::from_str(MINING_SUBSCRIBE_OK_RESULT_JSON).expect("BUG: Cannot prepare test result") {
        Rpc::Response(resp) => {
            let subscribe_result = SubscribeResult::try_from(resp).expect("BUG: Conversion failed");
            assert_eq!(
                expected_subscribe_result, subscribe_result,
                "Subscribe result mismatch"
            );
        }
        Rpc::Request(req) => {
            panic!("Received request ({:?} instead of response", req);
        } // Rpc::Unrecognized(value) => {
          //     panic!("Unrecongized message: {}", value);
          // }
    }
}

#[test]
#[should_panic]
fn test_subscribe_malformed_result_json() {
    match Rpc::from_str(MINING_SUBSCRIBE_MALFORMED_RESULT_JSON)
        .expect("BUG: Cannot prepare test result")
    {
        // This match arm should fail thus causing the test to pass
        Rpc::Response(resp) => {
            let _subscribe_result =
                SubscribeResult::try_from(resp).expect("BUG: Conversion failed");
        }
        // This match arm should not execute, if it does it is a bug and the test wouldn't panic
        // and would show up as failed
        Rpc::Request(_) => (),
    }
}
