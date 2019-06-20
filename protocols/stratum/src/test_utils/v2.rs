use slog::trace;
use std::fmt::Debug;
use uint;

use crate::test_utils::common::*;
use crate::v2::messages::*;
use crate::v2::types::*;
use crate::v2::{V2Handler, V2Protocol};
use crate::LOGGER;

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
        trace!(
            LOGGER,
            "V2 TestIdentityHandler: Message ID {:?} {:?}",
            msg.id,
            payload
        );
        assert_eq!(expected_payload, *payload, "Message payloads don't match");
    }
}

impl V2Handler for TestIdentityHandler {
    fn visit_setup_mining_connection(
        &mut self,
        msg: &wire::Message<V2Protocol>,
        payload: &SetupMiningConnection,
    ) {
        self.visit_and_check(msg, payload, build_setup_mining_connection);
    }

    fn visit_setup_mining_connection_success(
        &mut self,
        msg: &wire::Message<V2Protocol>,
        payload: &SetupMiningConnectionSuccess,
    ) {
        self.visit_and_check(msg, payload, build_setup_mining_connection_success);
    }
    fn visit_open_channel(&mut self, msg: &wire::Message<V2Protocol>, payload: &OpenChannel) {
        self.visit_and_check(msg, payload, build_open_channel);
    }
    fn visit_open_channel_success(
        &mut self,
        msg: &wire::Message<V2Protocol>,
        payload: &OpenChannelSuccess,
    ) {
        self.visit_and_check(msg, payload, build_open_channel_success);
    }
}

pub const SETUP_MINING_CONNECTION_SERIALIZED: &str =
    r#"{"protocol_version":0,"connection_url":"stratum.slushpool.com","required_extranonce_size":0}"#;

pub fn build_setup_mining_connection() -> SetupMiningConnection {
    SetupMiningConnection {
        protocol_version: 0,
        connection_url: POOL_URL.into(),
        required_extranonce_size: 0,
    }
}

pub const SETUP_MINING_CONNECTION_SUCCESS_SERIALIZED: &str =
    r#"{"protocol_version":0,"connection_url":"stratum.slushpool.com","required_extranonce_size":0}"#;

pub fn build_setup_mining_connection_success() -> SetupMiningConnectionSuccess {
    SetupMiningConnectionSuccess {
        used_protocol_version: 0,
        max_extranonce_size: 0,
        pub_key: vec![0xde, 0xad, 0xbe, 0xef],
    }
}

pub fn build_open_channel() -> OpenChannel {
    OpenChannel {
        req_id: 10,
        user: USER_CREDENTIALS.to_string(),
        extended: false,
        device: DeviceInfo {
            vendor: "Braiins".to_string(),
            hw_rev: "1".to_string(),
            fw_ver: MINER_SW_SIGNATURE.to_string(),
            dev_id: "xyz".to_string(),
        },
        nominal_hashrate: 1e9,
        // Maximum bitcoin target is 0xffff << 208 (= difficulty 1 share)
        max_target_nbits: 0x1d00ffff,
        aggregated_device_count: 1,
    }
}

pub fn build_open_channel_success() -> OpenChannelSuccess {
    let init_target_be = uint::U256::from_big_endian(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7f, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ]);
    let mut init_target_le = [0u8; 32];
    init_target_be.to_little_endian(&mut init_target_le);

    OpenChannelSuccess {
        req_id: 10,
        channel_id: 0,
        // don't provide device ID as the sample OpenChannel already provides one
        dev_id: None,
        // Represents difficulty 512
        init_target: Uint256Bytes(init_target_le),
        group_channel_id: 0,
    }
}
