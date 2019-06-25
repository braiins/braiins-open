use bitcoin_hashes::{hex::FromHex, sha256d, Hash};
use slog::trace;
use std::fmt::Debug;
use uint;

use crate::test_utils::common::*;
use crate::test_utils::v1::MINING_NOTIFY_MERKLE_ROOT;
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

    fn visit_new_mining_job(&mut self, msg: &wire::Message<V2Protocol>, payload: &NewMiningJob) {
        self.visit_and_check(msg, payload, build_new_mining_job);
    }

    fn visit_set_new_prev_hash(
        &mut self,
        msg: &wire::Message<V2Protocol>,
        payload: &SetNewPrevHash,
    ) {
        self.visit_and_check(msg, payload, build_set_new_prev_hash);
    }

    fn visit_submit_shares(&mut self, msg: &wire::Message<V2Protocol>, payload: &SubmitShares) {
        self.visit_and_check(msg, payload, build_submit_shares);
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

/// TODO: see test_utils::v1::MINING_NOTIFY_JSON that defines a stratum v1 job.
/// The merkle root below has been calculated by the integration test and cannot be trusted...
/// We need a V1 mining job with verified merkle root that is to be copied
pub fn build_new_mining_job() -> NewMiningJob {
    let expected_merkle_root =
        sha256d::Hash::from_hex(MINING_NOTIFY_MERKLE_ROOT).expect("from_hex");
    NewMiningJob {
        channel_id: 0,
        job_id: 0,
        block_height: 0,
        merkle_root: Uint256Bytes(expected_merkle_root.into_inner()),
        version: MINING_WORK_VERSION,
    }
}

pub fn build_set_new_prev_hash() -> SetNewPrevHash {
    // Extract the prevhash and other information from V1 message to prevent any duplication
    let v1_req = v1::build_mining_notify();
    let prev_hash = sha256d::Hash::from_slice(v1_req.prev_hash()).expect("Cannot build Prev Hash");

    SetNewPrevHash {
        block_height: 0,
        prev_hash: Uint256Bytes(prev_hash.into_inner()),
        min_ntime: v1_req.time(),
        // TODO: this needs to be reviewed and system time should be deterministically involved,
        // too?
        max_ntime_offset: 1800,
        nbits: v1_req.bits(),
    }
}

pub fn build_submit_shares() -> SubmitShares {
    // Use the mining job to provide sensible information for the share submit
    let mining_job = build_new_mining_job();

    SubmitShares {
        channel_id: mining_job.channel_id,
        seq_num: 0,
        job_id: mining_job.job_id,
        nonce: MINING_WORK_NONCE,
        ntime_offset: 0,
        version: MINING_WORK_VERSION,
    }
}
