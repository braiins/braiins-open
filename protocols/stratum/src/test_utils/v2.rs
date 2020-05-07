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

use bitcoin_hashes::{hex::FromHex, sha256d, Hash};
use std::fmt::Debug;
use uint;

use ii_logging::macros::*;
use ii_unvariant::handler;

use crate::error::Result;
use crate::test_utils::common::*;
use crate::test_utils::v1;
use crate::v2::{framing, messages::*, telemetry, types::*};

/// Message payload visitor that compares the payload of the visited message (e.g. after
/// deserialization test) with the payload built.
/// This handler should be used in tests to verify that serialization and deserialization yield the
/// same results
pub struct TestIdentityHandler;

impl TestIdentityHandler {
    #[inline]
    fn check_payload<P, F>(&self, payload_result: Result<P>, build: F)
    where
        P: Debug + PartialEq,
        F: FnOnce() -> P,
    {
        // Build expected payload for verifying correct deserialization
        let payload = payload_result.expect("BUG: Message parsing failed");
        let expected_payload = build();
        trace!("V2 TestIdentityHandler: Message {:?}", payload);
        assert_eq!(expected_payload, payload, "Message payloads don't match");
    }
}

#[handler(async try framing::Frame suffix _v2)]
impl TestIdentityHandler {
    async fn handle_setup_connection(&mut self, msg: Result<SetupConnection>) {
        self.check_payload(msg, build_setup_connection);
    }

    async fn handle_setup_connection_success(&mut self, msg: Result<SetupConnectionSuccess>) {
        self.check_payload(msg, build_setup_connection_success);
    }

    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: Result<OpenStandardMiningChannel>,
    ) {
        self.check_payload(msg, build_open_channel);
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: Result<OpenStandardMiningChannelSuccess>,
    ) {
        self.check_payload(msg, build_open_channel_success);
    }

    async fn handle_new_mining_job(&mut self, msg: Result<NewMiningJob>) {
        self.check_payload(msg, build_new_mining_job);
    }

    async fn handle_set_new_prev_hash(&mut self, msg: Result<SetNewPrevHash>) {
        self.check_payload(msg, build_set_new_prev_hash);
    }

    async fn handle_submit_shares_standard(&mut self, msg: Result<SubmitSharesStandard>) {
        self.check_payload(msg, build_submit_shares);
    }

    async fn handle_submit_shares_success(&mut self, _msg: Result<SubmitSharesSuccess>) {
        // self.check_payload(msg, build_submit_shares);
        // TODO
    }

    async fn handle_reconnect(&mut self, msg: Result<Reconnect>) {
        self.check_payload(msg, build_reconnect);
    }

    #[handle(_)]
    async fn handle_everything(&mut self, frame: framing::Frame) {
        panic!("BUG: No handler method for received frame: {:?}", frame);
    }
}

#[cfg(not(feature = "v2json"))]
pub const SETUP_CONNECTION_SERIALIZED: &'static [u8] =
    b"\x00\x02\x00\x02\x00\x00\x00\x00\x00\x15stratum.slushpool.com\x05\x0d\x07Braiins\x011\x15Braiins OS 2019-06-05\x03xyz";
#[cfg(feature = "v2json")]
pub const SETUP_CONNECTION_SERIALIZED: &'static [u8] =
    br#"{"max_version":2,"min_version":2,"flags":0,"expected_pubkey":[],"endpoint_host":"stratum.slushpool.com","endpoint_port":3333,"device":{"vendor":"Braiins","hw_rev":"1","fw_ver":"Braiins OS 2019-06-05","dev_id":"xyz"}}"#;

pub fn build_setup_connection() -> SetupConnection {
    SetupConnection {
        protocol: 0,
        max_version: 2,
        min_version: 2,
        flags: 0,
        endpoint_host: Str0_255::from_str(POOL_URL),
        endpoint_port: POOL_PORT as u16,
        device: DeviceInfo {
            vendor: "Braiins".try_into().unwrap(),
            hw_rev: "1".try_into().unwrap(),
            fw_ver: MINER_SW_SIGNATURE.try_into().unwrap(),
            dev_id: "xyz".try_into().unwrap(),
        },
    }
}

pub const SETUP_CONNECTION_SUCCESS_SERIALIZED: &'static [u8] =
    br#"{"protocol_version":0,"connection_url":"stratum.slushpool.com","required_extranonce_size":0}"#;

pub fn build_setup_connection_success() -> SetupConnectionSuccess {
    SetupConnectionSuccess {
        used_version: 0,
        flags: 0,
    }
}

pub fn build_open_channel() -> OpenStandardMiningChannel {
    OpenStandardMiningChannel {
        req_id: 10,
        user: USER_CREDENTIALS.try_into().unwrap(),
        nominal_hashrate: 1e9,
        max_target: ii_bitcoin::Target::default().into(),
    }
}

pub fn build_open_channel_success() -> OpenStandardMiningChannelSuccess {
    let init_target_be = uint::U256::from_big_endian(&[
        0x00, 0x00, 0x00, 0x00, 0x3f, 0xff, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ]);
    let mut init_target_le = [0u8; 32];
    init_target_be.to_little_endian(&mut init_target_le);

    OpenStandardMiningChannelSuccess {
        req_id: 10,
        channel_id: 0,
        // Represents difficulty 4
        target: Uint256Bytes(init_target_le),
        extranonce_prefix: Bytes0_32::new(),
        group_channel_id: 0,
    }
}

/// TODO: see test_utils::v1::MINING_NOTIFY_JSON that defines a stratum v1 job.
/// The merkle root below has been calculated by the integration test and cannot be trusted...
/// We need a V1 mining job with verified merkle root that is to be copied
pub fn build_new_mining_job() -> NewMiningJob {
    let expected_merkle_root =
        sha256d::Hash::from_hex(v1::MINING_NOTIFY_MERKLE_ROOT).expect("from_hex");
    NewMiningJob {
        channel_id: 0,
        job_id: 0,
        future_job: true,
        merkle_root: Uint256Bytes(expected_merkle_root.into_inner()),
        version: MINING_WORK_VERSION,
    }
}

pub fn build_set_new_prev_hash() -> SetNewPrevHash {
    // Extract the prevhash and other information from V1 message to prevent any duplication
    let v1_req = v1::build_mining_notify();
    let prev_hash = sha256d::Hash::from_slice(v1_req.prev_hash()).expect("Cannot build Prev Hash");

    SetNewPrevHash {
        channel_id: 0,
        prev_hash: Uint256Bytes(prev_hash.into_inner()),
        min_ntime: v1_req.time(),
        job_id: 0,
        nbits: v1_req.bits(),
    }
}

pub fn build_submit_shares() -> SubmitSharesStandard {
    // Use the mining job to provide sensible information for the share submit
    let mining_job = build_new_mining_job();

    SubmitSharesStandard {
        channel_id: mining_job.channel_id,
        seq_num: 0,
        job_id: mining_job.job_id,
        nonce: MINING_WORK_NONCE,
        ntime: MINING_WORK_NTIME,
        version: MINING_WORK_VERSION,
    }
}

pub fn build_reconnect() -> Reconnect {
    Reconnect {
        new_host: Str0_255::from_str(POOL_URL),
        new_port: POOL_PORT as u16,
    }
}

pub fn build_open_telemetry_channel() -> telemetry::messages::OpenTelemetryChannel {
    telemetry::messages::OpenTelemetryChannel {
        req_id: 0,
        dev_id: Default::default(),
    }
}

pub fn build_open_telemetry_channel_success() -> telemetry::messages::OpenTelemetryChannelSuccess {
    telemetry::messages::OpenTelemetryChannelSuccess {
        req_id: 0,
        channel_id: 0,
    }
}
pub fn build_open_telemetry_channel_error() -> telemetry::messages::OpenTelemetryChannelError {
    telemetry::messages::OpenTelemetryChannelError {
        req_id: 0,
        code: Default::default(),
    }
}
pub fn build_submit_telemetry_data() -> telemetry::messages::SubmitTelemetryData {
    telemetry::messages::SubmitTelemetryData {
        channel_id: 0,
        seq_num: 0,
        telemetry_payload: Default::default(),
    }
}
pub fn build_submit_telemetry_data_success() -> telemetry::messages::SubmitTelemetryDataSuccess {
    telemetry::messages::SubmitTelemetryDataSuccess {
        channel_id: 0,
        last_seq_num: 0,
    }
}
pub fn build_submit_telemetry_data_error() -> telemetry::messages::SubmitTelemetryDataError {
    telemetry::messages::SubmitTelemetryDataError {
        channel_id: 0,
        seq_num: 0,
        code: Default::default(),
    }
}
