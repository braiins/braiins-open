use crate::v2::messages::*;
use crate::v2::{V2Handler, V2Protocol};
use std::fmt::Debug;

/// Message payload visitor that compares the payload of the visited message (e.g. after
/// deserialization test) with the payload built.
/// This handler should be used in tests to verify that serialization and deserialization yield the
/// same results
pub struct TestIdentityHandler;

impl TestIdentityHandler {
    fn visit_and_check<P, F>(&self, msg: &wire::Message<V2Protocol>, payload: &P, build: F)
    where
        P: Debug + PartialEq,
        F: FnOnce() -> P,
    {
        // Build expected payload for verifying correct deserialization
        let expected_payload = build();
        println!("Message ID {:?} {:?}", msg.id, payload);
        assert_eq!(expected_payload, *payload, "Message payloads don't match");
    }
}

impl V2Handler for TestIdentityHandler {
    fn visit_setup_mining_connection(
        &self,
        msg: &wire::Message<V2Protocol>,
        payload: &SetupMiningConnection,
    ) {
        self.visit_and_check(msg, payload, build_setup_mining_connection);
    }

    fn visit_setup_mining_connection_success(
        &self,
        msg: &wire::Message<V2Protocol>,
        payload: &SetupMiningConnectionSuccess,
    ) {
        self.visit_and_check(msg, payload, build_setup_mining_connection_success);
    }
}

pub const SETUP_MINING_CONNECTION_SERIALIZED: &str =
    r#"{"protocol_version":0,"connection_url":"test.pool","required_extranonce_size":4}"#;

pub fn build_setup_mining_connection() -> SetupMiningConnection {
    SetupMiningConnection {
        protocol_version: 0,
        connection_url: "test.pool".into(),
        required_extranonce_size: 4,
    }
}

pub const SETUP_MINING_CONNECTION_SUCCESS_SERIALIZED: &str =
    r#"{"protocol_version":0,"connection_url":"test.pool","required_extranonce_size":4}"#;

pub fn build_setup_mining_connection_success() -> SetupMiningConnectionSuccess {
    SetupMiningConnectionSuccess {
        used_protocol_version: 0,
        max_extranonce_size: 4,
        pub_key: vec![0xde, 0xad, 0xbe, 0xef],
    }
}
