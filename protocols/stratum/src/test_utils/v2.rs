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
use bitcoin_hashes::{hex::FromHex, sha256d, Hash};
use std::collections::VecDeque;
use std::fmt::Debug;
use uint;

use ii_logging::macros::*;
use ii_unvariant::handler;

use crate::error::Result;
use crate::test_utils::common::*;
use crate::test_utils::v1;
use crate::v2::{framing, messages::*, telemetry, types::*};

pub enum TestMessage {
    MsgSetupConnection(SetupConnection),
    MsgSetupConnectionSuccess(SetupConnectionSuccess),
    MsgSetupConnectionError(SetupConnectionError),
    MsgChannelEndpointChanged(ChannelEndpointChanged),
    MsgOpenStandardMiningChannel(OpenStandardMiningChannel),
    MsgOpenStandardMiningChannelSuccess(OpenStandardMiningChannelSuccess),
    MsgOpenMiningChannelError(OpenMiningChannelError),
    MsgUpdateChannel(UpdateChannel),
    MsgUpdateChannelError(UpdateChannelError),
    MsgCloseChannel(CloseChannel),
    MsgSubmitSharesStandard(SubmitSharesStandard),
    MsgSubmitSharesSuccess(SubmitSharesSuccess),
    MsgSubmitSharesError(SubmitSharesError),
    MsgNewMiningJob(NewMiningJob),
    MsgNewExtendedMiningJob(NewExtendedMiningJob),
    MsgSetNewPrevHash(SetNewPrevHash),
    MsgSetTarget(SetTarget),
    MsgReconnect(Reconnect),
}

macro_rules! impl_unwrap {
    ($method:ident, $from_enum:ident, $to_msg:ident) => {
        pub fn $method(self) -> $to_msg {
            match self {
                Self::$from_enum(msg) => msg,
                _ => panic!("BUG: expected '{}'", stringify!($to_msg)),
            }
        }
    };
}

impl TestMessage {
    impl_unwrap!(unwrap_setup_connection, MsgSetupConnection, SetupConnection);
    impl_unwrap!(
        unwrap_setup_connection_success,
        MsgSetupConnectionSuccess,
        SetupConnectionSuccess
    );
    impl_unwrap!(
        unwrap_setup_connection_error,
        MsgSetupConnectionError,
        SetupConnectionError
    );
    impl_unwrap!(
        unwrap_channel_endpoint_changed,
        MsgChannelEndpointChanged,
        ChannelEndpointChanged
    );
    impl_unwrap!(
        unwrap_open_standard_mining_channel,
        MsgOpenStandardMiningChannel,
        OpenStandardMiningChannel
    );
    impl_unwrap!(
        unwrap_open_standard_mining_channel_success,
        MsgOpenStandardMiningChannelSuccess,
        OpenStandardMiningChannelSuccess
    );
    impl_unwrap!(
        unwrap_open_mining_channel_error,
        MsgOpenMiningChannelError,
        OpenMiningChannelError
    );
    impl_unwrap!(unwrap_update_channel, MsgUpdateChannel, UpdateChannel);
    impl_unwrap!(
        unwrap_update_channel_error,
        MsgUpdateChannelError,
        UpdateChannelError
    );
    impl_unwrap!(unwrap_close_channel, MsgCloseChannel, CloseChannel);
    impl_unwrap!(
        unwrap_submit_shares_standard,
        MsgSubmitSharesStandard,
        SubmitSharesStandard
    );
    impl_unwrap!(
        unwrap_submit_shares_success,
        MsgSubmitSharesSuccess,
        SubmitSharesSuccess
    );
    impl_unwrap!(
        unwrap_submit_shares_error,
        MsgSubmitSharesError,
        SubmitSharesError
    );
    impl_unwrap!(unwrap_new_mining_job, MsgNewMiningJob, NewMiningJob);
    impl_unwrap!(
        unwrap_new_extended_mining_job,
        MsgNewExtendedMiningJob,
        NewExtendedMiningJob
    );
    impl_unwrap!(unwrap_set_new_prev_hash, MsgSetNewPrevHash, SetNewPrevHash);
    impl_unwrap!(unwrap_set_target, MsgSetTarget, SetTarget);
    impl_unwrap!(unwrap_reconnect, MsgReconnect, Reconnect);
}

macro_rules! impl_from_msg_to_enum {
    ($from_msg:ident, $to_enum:ident) => {
        impl From<$from_msg> for TestMessage {
            fn from(msg: $from_msg) -> Self {
                Self::$to_enum(msg)
            }
        }
    };
}

macro_rules! impl_try_from_enum_to_msg {
    ($from_enum:ident, $to_msg:ident) => {
        impl TryFrom<TestMessage> for $to_msg {
            type Error = ();

            fn try_from(msg: TestMessage) -> std::result::Result<Self, Self::Error> {
                match msg {
                    TestMessage::$from_enum(msg) => Ok(msg),
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

impl_conversions!(SetupConnection, MsgSetupConnection);
impl_conversions!(SetupConnectionSuccess, MsgSetupConnectionSuccess);
impl_conversions!(SetupConnectionError, MsgSetupConnectionError);
impl_conversions!(ChannelEndpointChanged, MsgChannelEndpointChanged);
impl_conversions!(OpenStandardMiningChannel, MsgOpenStandardMiningChannel);
impl_conversions!(
    OpenStandardMiningChannelSuccess,
    MsgOpenStandardMiningChannelSuccess
);
impl_conversions!(OpenMiningChannelError, MsgOpenMiningChannelError);
impl_conversions!(UpdateChannel, MsgUpdateChannel);
impl_conversions!(UpdateChannelError, MsgUpdateChannelError);
impl_conversions!(CloseChannel, MsgCloseChannel);
impl_conversions!(SubmitSharesStandard, MsgSubmitSharesStandard);
impl_conversions!(SubmitSharesSuccess, MsgSubmitSharesSuccess);
impl_conversions!(SubmitSharesError, MsgSubmitSharesError);
impl_conversions!(NewMiningJob, MsgNewMiningJob);
impl_conversions!(NewExtendedMiningJob, MsgNewExtendedMiningJob);
impl_conversions!(SetNewPrevHash, MsgSetNewPrevHash);
impl_conversions!(SetTarget, MsgSetTarget);
impl_conversions!(Reconnect, MsgReconnect);

#[derive(Default)]
pub struct TestCollectorHandler {
    messages: VecDeque<TestMessage>,
}

#[handler(async try framing::Frame suffix _v2)]
impl TestCollectorHandler {
    async fn handle_setup_connection(&mut self, msg: SetupConnection) {
        self.messages.push_back(msg.into());
    }

    async fn handle_setup_connection_success(&mut self, msg: SetupConnectionSuccess) {
        self.messages.push_back(msg.into());
    }

    async fn handle_setup_connection_error(&mut self, msg: SetupConnectionError) {
        self.messages.push_back(msg.into());
    }

    async fn handle_channel_endpoint_changed(&mut self, msg: ChannelEndpointChanged) {
        self.messages.push_back(msg.into());
    }

    async fn handle_open_standard_mining_channel(&mut self, msg: OpenStandardMiningChannel) {
        self.messages.push_back(msg.into());
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess,
    ) {
        self.messages.push_back(msg.into());
    }

    async fn handle_open_mining_channel_error(&mut self, msg: OpenMiningChannelError) {
        self.messages.push_back(msg.into());
    }

    async fn handle_update_channel(&mut self, msg: UpdateChannel) {
        self.messages.push_back(msg.into());
    }

    async fn handle_update_channel_error(&mut self, msg: UpdateChannelError) {
        self.messages.push_back(msg.into());
    }

    async fn handle_close_channel(&mut self, msg: CloseChannel) {
        self.messages.push_back(msg.into());
    }

    async fn handle_submit_shares_standard(&mut self, msg: SubmitSharesStandard) {
        self.messages.push_back(msg.into());
    }

    async fn handle_submit_shares_success(&mut self, msg: SubmitSharesSuccess) {
        self.messages.push_back(msg.into());
    }

    async fn handle_submit_shares_error(&mut self, msg: SubmitSharesError) {
        self.messages.push_back(msg.into());
    }

    async fn handle_new_mining_job(&mut self, msg: NewMiningJob) {
        self.messages.push_back(msg.into());
    }

    async fn handle_new_extended_mining_job(&mut self, msg: NewExtendedMiningJob) {
        self.messages.push_back(msg.into());
    }

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash) {
        self.messages.push_back(msg.into());
    }

    async fn handle_set_target(&mut self, msg: SetTarget) {
        self.messages.push_back(msg.into());
    }

    async fn handle_reconnect(&mut self, msg: Reconnect) {
        self.messages.push_back(msg.into());
    }

    #[handle(_)]
    async fn handle_everything(&mut self, frame: Result<framing::Frame>) {
        let frame = frame.expect("BUG: Message parsing failed");
        panic!("BUG: No handler method for received frame: {:?}", frame);
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
    async fn receive_v2(&mut self) -> framing::Frame;

    async fn next_v2(&mut self) -> TestMessage {
        let frame = self.receive_v2().await;
        let mut handler = TestCollectorHandler::default();
        handler.handle_v2(frame).await;
        handler.next().expect("BUG: No message was received")
    }

    async fn check_next_v2<T, U, V>(&mut self, f: T) -> V
    where
        T: FnOnce(U) -> V + Send + Sync,
        U: TryFrom<TestMessage, Error = ()>,
    {
        let msg = self.next_v2().await;
        f(U::try_from(msg).expect(format!("BUG: expected '{}'", stringify!(U)).as_str()))
    }
}

pub fn message_check<P>(payload: P, expected_payload: P)
where
    P: Debug + PartialEq,
{
    trace!("V2: Message {:?}", payload);
    assert_eq!(expected_payload, payload, "Message payloads don't match");
}

/// Message payload visitor that compares the payload of the visited message (e.g. after
/// deserialization test) with the payload built.
/// This handler should be used in tests to verify that serialization and deserialization yield the
/// same results
pub struct TestIdentityHandler;

#[handler(async try framing::Frame suffix _v2)]
impl TestIdentityHandler {
    async fn handle_setup_connection(&mut self, msg: SetupConnection) {
        message_check(msg, build_setup_connection());
    }

    async fn handle_setup_connection_success(&mut self, msg: SetupConnectionSuccess) {
        message_check(msg, build_setup_connection_success());
    }

    async fn handle_open_standard_mining_channel(&mut self, msg: OpenStandardMiningChannel) {
        message_check(msg, build_open_channel());
    }

    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: OpenStandardMiningChannelSuccess,
    ) {
        message_check(msg, build_open_channel_success());
    }

    async fn handle_new_mining_job(&mut self, msg: NewMiningJob) {
        message_check(msg, build_new_mining_job());
    }

    async fn handle_set_new_prev_hash(&mut self, msg: SetNewPrevHash) {
        message_check(msg, build_set_new_prev_hash());
    }

    async fn handle_submit_shares_standard(&mut self, msg: SubmitSharesStandard) {
        message_check(msg, build_submit_shares());
    }

    async fn handle_submit_shares_success(&mut self, msg: SubmitSharesSuccess) {
        message_check(msg, build_submit_shares_success());
    }

    async fn handle_reconnect(&mut self, msg: Reconnect) {
        message_check(msg, build_reconnect());
    }

    #[handle(_)]
    async fn handle_everything(&mut self, frame: Result<framing::Frame>) {
        let frame = frame.expect("BUG: Message parsing failed");
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
            vendor: Str0_255::from_str("Braiins"),
            hw_rev: Str0_255::from_str("1"),
            fw_ver: Str0_255::from_str(MINER_SW_SIGNATURE),
            dev_id: Str0_255::from_str("xyz"),
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
        user: Str0_255::from_str(USER_CREDENTIALS),
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
        sha256d::Hash::from_hex(v1::MINING_NOTIFY_MERKLE_ROOT).expect("BUG: from_hex");
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
    let prev_hash =
        sha256d::Hash::from_slice(v1_req.prev_hash()).expect("BUG: Cannot build Prev Hash");

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

pub fn build_submit_shares_success() -> SubmitSharesSuccess {
    SubmitSharesSuccess {
        channel_id: 0,
        last_seq_num: 0,
        new_submits_accepted_count: 1,
        new_shares_sum: 0,
    }
}

pub fn build_submit_shares_error() -> SubmitSharesError {
    SubmitSharesError {
        channel_id: 0,
        seq_num: 0,
        code: Str0_32::from_str(""),
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
