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

//! All stratum V2 protocol messages

use serde;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use super::extensions;
use super::framing;
#[cfg(not(feature = "v2json"))]
use super::serialization;
use super::types::*;
use super::Protocol;
use crate::error::{Error, Result};
use crate::AnyPayload;
#[cfg(feature = "v2json")]
use serde_json as serialization;

use ii_unvariant::{id, Id};

#[cfg(test)]
mod test;

/// Generates conversion for base protocol messages (extension 0)
macro_rules! impl_base_message_conversion {
    ($message:tt, $is_channel_msg:expr, $handler_fn:tt) => {
        impl_message_conversion!(extensions::BASE, $message, $is_channel_msg, $handler_fn);
    };
}

#[id(0x00u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupConnection {
    pub protocol: u8,
    pub min_version: u16,
    pub max_version: u16,
    /// TODO: specify an enum for flags
    pub flags: u32,
    pub endpoint_host: Str0_255,
    pub endpoint_port: u16,
    pub device: DeviceInfo,
}

#[id(0x01u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupConnectionSuccess {
    pub used_version: u16,
    /// TODO: specify an enum for flags
    pub flags: u32,
}

#[id(0x02u8)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetupConnectionError {
    pub flags: u32,
    pub code: Str0_255,
}

#[id(0x03u8)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChannelEndpointChanged {
    pub channel_id: u32,
}

#[id(0x10u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenStandardMiningChannel {
    pub req_id: u32,
    pub user: Str0_255,
    pub nominal_hashrate: f32,
    pub max_target: Uint256Bytes,
}

#[id(0x11u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenStandardMiningChannelSuccess {
    pub req_id: u32,
    pub channel_id: u32,
    /// Initial target for mining
    pub target: Uint256Bytes,
    pub extranonce_prefix: Bytes0_32,
    /// See SetGroupChannel for details
    pub group_channel_id: u32,
}

#[id(0x12u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenStandardMiningChannelError {
    pub req_id: u32,
    pub code: Str0_32,
}

#[id(0x16u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UpdateChannel {
    pub channel_id: u32,
    pub nominal_hash_rate: f32,
    pub maximum_target: Uint256Bytes,
}

#[id(0x17u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UpdateChannelError {
    pub channel_id: u32,
    pub error_code: Str0_32,
}

#[id(0x18u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CloseChannel {
    pub channel_id: u32,
    pub reason_code: Str0_32,
}

#[id(0x1au8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesStandard {
    pub channel_id: u32,
    pub seq_num: u32,
    pub job_id: u32,
    pub nonce: u32,
    pub ntime: u32,
    pub version: u32,
}

#[id(0x1cu8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesSuccess {
    pub channel_id: u32,
    pub last_seq_num: u32,
    pub new_submits_accepted_count: u32,
    pub new_shares_sum: u32,
}

#[id(0x1du8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesError {
    pub channel_id: u32,
    pub seq_num: u32,
    pub code: Str0_32,
}

#[id(0x1eu8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewMiningJob {
    pub channel_id: u32,
    pub job_id: u32,
    pub future_job: bool,
    pub version: u32,
    pub merkle_root: Uint256Bytes,
}

#[id(0x1fu8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewExtendedMiningJob {
    pub channel_id: u32,
    pub job_id: u32,
    pub future_job: bool,
    pub version: u32,
    pub version_rolling_allowed: bool,
    pub merkle_path: Seq0_255<Uint256Bytes>,
    pub coinbase_tx_prefix: Bytes0_64k,
    pub coinbase_tx_suffix: Bytes0_64k,
}

#[id(0x20u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetNewPrevHash {
    pub channel_id: u32,
    pub job_id: u32,
    pub prev_hash: Uint256Bytes,
    pub min_ntime: u32,
    pub nbits: u32,
}

pub struct SetCustomMiningJob;
pub struct SetCustomMiningJobSuccess;

#[id(0x21u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetTarget {
    pub channel_id: u32,
    pub max_target: Uint256Bytes,
}

#[id(0x25u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Reconnect {
    pub new_host: Str0_255,
    pub new_port: u16,
}

pub struct SetGroupChannel;

impl_base_message_conversion!(SetupConnection, false, visit_setup_connection);
impl_base_message_conversion!(
    SetupConnectionSuccess,
    false,
    visit_setup_connection_success
);
impl_base_message_conversion!(SetupConnectionError, false, visit_setup_connection_error);
impl_base_message_conversion!(
    ChannelEndpointChanged,
    false,
    visit_channel_endpoint_changed
);
impl_base_message_conversion!(
    OpenStandardMiningChannel,
    false,
    visit_open_standard_mining_channel
);
impl_base_message_conversion!(
    OpenStandardMiningChannelSuccess,
    false,
    visit_open_standard_mining_channel_success
);
impl_base_message_conversion!(
    OpenStandardMiningChannelError,
    false,
    visit_open_standard_mining_channel_error
);

impl_base_message_conversion!(UpdateChannel, true, visit_update_channel);
impl_base_message_conversion!(UpdateChannelError, true, visit_update_channel_error);
impl_base_message_conversion!(CloseChannel, true, visit_close_channel);
impl_base_message_conversion!(SubmitSharesStandard, true, visit_submit_shares_standard);
impl_base_message_conversion!(SubmitSharesSuccess, true, visit_submit_shares_success);
impl_base_message_conversion!(SubmitSharesError, true, visit_submit_shares_error);
impl_base_message_conversion!(NewMiningJob, true, visit_new_mining_job);
impl_base_message_conversion!(NewExtendedMiningJob, true, visit_new_extended_mining_job);
impl_base_message_conversion!(SetNewPrevHash, true, visit_set_new_prev_hash);
impl_base_message_conversion!(Reconnect, false, visit_reconnect);
impl_base_message_conversion!(SetTarget, true, visit_set_target);
