// Copyright (C) 2021  Braiins Systems s.r.o.
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
    ($message:tt, $is_channel_msg:expr) => {
        impl_message_conversion!(extensions::BASE, $message, $is_channel_msg);
    };
}

/// Initiates the connection. This MUST be the first message sent by the client on the newly opened
/// connection. Server MUST respond with either a [`SetupConnectionSuccess`] or [`SetupConnectionError`]
/// message. Clients that are not configured to provide telemetry data to the upstream node SHOULD
/// set device_id to 0-length strings. However, they MUST always set vendor to a string describing
/// the manufacturer/developer and firmware version and SHOULD always set hardware_version to a
/// string describing, at least, the particular hardware/software package in use.
#[id(0x00u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupConnection {
    /// 0 = Mining Protocol
    /// 1 = Job Negotiation Protocol
    /// 2 = Template Distribution Protocol
    /// 3 = Job Distribution Protocol
    pub protocol: u8,
    /// The minimum protocol version the client supports (currently must be 2).
    pub min_version: u16,
    /// The maximum protocol version the client supports (currently must be 2).
    pub max_version: u16,
    // TODO: specify an enum for flags
    /// Flags indicating optional protocol features the client supports. Each protocol from protocol
    /// field has its own values/flags.
    pub flags: u32,
    /// ASCII text indicating the hostname or IP address (upstream host).
    pub endpoint_host: Str0_255,
    /// Connecting port value (upstream port).
    pub endpoint_port: u16,
    pub device: DeviceInfo,
}

/// Response to SetupConnection message if the server accepts the connection. The client is required
/// to verify the set of feature flags that the server supports and act accordingly.
#[id(0x01u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupConnectionSuccess {
    /// Selected version proposed by the connecting node that the upstream node supports. This version will be used on the connection for the rest of its life.
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
    /// Client-specified identifier for matching responses from upstream server.
    /// The value MUST be connection-wide unique and is not interpreted by the server.
    pub req_id: u32,
    /// Unconstrained sequence of bytes. Whatever is needed by upstream node to
    /// identify/authenticate the client, e.g. “braiinstest.worker1”.
    /// Additional restrictions can be imposed by the upstream node (e.g. a pool).
    /// It is highly recommended that UTF-8 encoding is used.
    pub user: Str0_255,
    /// [h/s] Expected hash rate of the device (or cumulative hashrate on the channel if multiple
    /// devices are connected downstream) in h/s. Depending on server’s target setting policy,
    /// this value can be used for setting a reasonable target for the channel.
    /// Proxy MUST send 0.0f when there are no mining devices connected yet.
    pub nominal_hashrate: f32,
    /// Maximum target which can be accepted by the connected device or devices.
    /// Server MUST accept the target or respond by sending [`OpenMiningChannelError`] message.
    pub max_target: Uint256Bytes,
}

/// Similar to [`OpenStandardMiningChannel`] but requests to open an extended channel instead of
/// standard channel.
#[id(0x13u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenExtendedMiningChannel {
    /// Client-specified identifier for matching responses from upstream server.
    /// The value MUST be connection-wide unique and is not interpreted by the server.
    pub req_id: u32,
    /// Unconstrained sequence of bytes. Whatever is needed by upstream node to
    /// identify/authenticate the client, e.g. “braiinstest.worker1”.
    /// Additional restrictions can be imposed by the upstream node (e.g. a pool).
    /// It is highly recommended that UTF-8 encoding is used.
    pub user: Str0_255,
    /// [h/s] Expected hash rate of the device (or cumulative hashrate on the channel if multiple
    /// devices are connected downstream) in h/s. Depending on server’s target setting policy,
    /// this value can be used for setting a reasonable target for the channel.
    /// Proxy MUST send 0.0f when there are no mining devices connected yet.
    pub nominal_hashrate: f32,
    /// Maximum target which can be accepted by the connected device or devices.
    /// Server MUST accept the target or respond by sending [`OpenMiningChannelError`] message.
    pub max_target: Uint256Bytes,
    /// Minimum size of extranonce needed by the device/node.
    pub min_extranonce_size: u16,
}

#[id(0x14u8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenExtendedMiningChannelSuccess {
    /// Client-specified request ID from [`OpenExtendedMiningChannel`] message, so that the client can
    /// pair responses with open channel requests.
    pub request_id: u32,
    /// Newly assigned identifier of the channel, stable for the whole lifetime of the connection.
    /// E.g. it is used for broadcasting new jobs by [`NewExtendedMiningJob`].
    pub channel_id: u32,
    /// Initial target for the mining channel.
    pub target: Uint256Bytes,
    /// Extranonce size (in bytes) set for the channel.
    pub extranonce_size: u16,
    /// Bytes used as implicit first part of extranonce.
    pub extranonce_prefix: Bytes0_32,
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
pub struct OpenMiningChannelError {
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
    /// Channel identification.
    pub channel_id: u32,
    /// Unique sequential identifier of the submit within the channel.
    pub seq_num: u32,
    /// Identifier of the job as provided by NewMiningJob or NewExtendedMiningJob message.
    pub job_id: u32,
    /// Nonce leading to the hash being submitted.
    pub nonce: u32,
    /// The nTime field in the block header. This MUST be greater than or equal to the
    /// header_timestamp field in the latest SetNewPrevHash message and lower than or equal to
    /// that value plus the number of seconds since the receipt of that message.
    pub ntime: u32,
    /// Full nVersion field.
    pub version: u32,
}

#[id(0x1bu8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesExtended {
    /// Channel identification.
    pub channel_id: u32,
    /// Unique sequential identifier of the submit within the channel.
    pub seq_num: u32,
    /// Identifier of the job as provided by NewMiningJob or NewExtendedMiningJob message.
    pub job_id: u32,
    /// Nonce leading to the hash being submitted.
    pub nonce: u32,
    /// The nTime field in the block header. This MUST be greater than or equal to the
    /// header_timestamp field in the latest SetNewPrevHash message and lower than or equal to
    /// that value plus the number of seconds since the receipt of that message.
    pub ntime: u32,
    /// Full nVersion field.
    pub version: u32,
    /// Extranonce bytes which need to be added to coinbase to form a fully valid submission
    /// (full coinbase = coinbase_tx_prefix + extranonce_prefix + extranonce + coinbase_tx_suffix).
    /// The size of the provided extranonce MUST be equal to the negotiated extranonce size from
    /// channel opening
    pub extranonce: Bytes0_32,
}

/// Response to SubmitShares or SubmitSharesExtended, accepting results from the miner.
/// Because it is a common case that shares submission is successful, this response can be
/// provided for multiple SubmitShare messages aggregated together.
#[id(0x1cu8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesSuccess {
    /// Channel identifier.
    pub channel_id: u32,
    /// Most recent sequence number with a correct result
    pub last_seq_num: u32,
    /// Most recent sequence number with a correct result
    pub new_submits_accepted_count: u32,
    /// Most recent sequence number with a correct result.
    pub new_shares_sum: u32,
}

/// An error is immediately submitted for every incorrect submit attempt. In case the server is
/// not able to immediately validate the submission, the error is sent as soon as the result is
/// known. This delayed validation can occur when a miner gets faster updates about a new prevhash
/// than the server does (see NewPrevHash message for details).
#[id(0x1du8)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesError {
    /// Channel identifier.
    pub channel_id: u32,
    /// Submission sequence number for which this error is returned.
    pub seq_num: u32,
    /// Human-readable error code(s). See Error Codes section, below
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
    /// For a group channel, the message is broadcasted to all standard channels belonging to the
    /// group. Otherwise, it is addressed to the specified extended channel.
    pub channel_id: u32,
    /// Server’s identification of the mining job.
    pub job_id: u32,
    /// True if the job is intended for a future SetNewPrevHash message sent on the channel.  If
    /// False, the job relates to the last sent SetNewPrevHash message on the channel and the miner
    /// should start to work on the job immediately.
    pub future_job: bool,
    /// Valid version field that reflects the current network consensus.
    pub version: u32,
    /// If set to True, the general purpose bits of version (as specified in BIP320) can be freely
    /// manipulated by the downstream node. The downstream node MUST NOT rely on the upstream node
    /// to set the BIP320 bits to any particular value. If set to False, the downstream node MUST
    /// use version as it is defined by this message.
    pub version_rolling_allowed: bool,
    /// Merkle path hashes ordered from deepest.
    pub merkle_path: Seq0_255<Uint256Bytes>,
    /// Prefix part of the coinbase transaction*.
    pub coinbase_tx_prefix: Bytes0_64k,
    /// Suffix part of the coinbase transaction.
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

impl_base_message_conversion!(SetupConnection, false);
impl_base_message_conversion!(SetupConnectionSuccess, false);
impl_base_message_conversion!(SetupConnectionError, false);
impl_base_message_conversion!(ChannelEndpointChanged, false);
impl_base_message_conversion!(OpenStandardMiningChannel, false);
impl_base_message_conversion!(OpenExtendedMiningChannel, false);
impl_base_message_conversion!(OpenStandardMiningChannelSuccess, false);
impl_base_message_conversion!(OpenExtendedMiningChannelSuccess, false);
impl_base_message_conversion!(OpenMiningChannelError, false);

impl_base_message_conversion!(UpdateChannel, true);
impl_base_message_conversion!(UpdateChannelError, true);
impl_base_message_conversion!(CloseChannel, true);
impl_base_message_conversion!(SubmitSharesStandard, true);
impl_base_message_conversion!(SubmitSharesExtended, true);
impl_base_message_conversion!(SubmitSharesSuccess, true);
impl_base_message_conversion!(SubmitSharesError, true);
impl_base_message_conversion!(NewMiningJob, true);
impl_base_message_conversion!(NewExtendedMiningJob, true);
impl_base_message_conversion!(SetNewPrevHash, true);
impl_base_message_conversion!(Reconnect, false);
impl_base_message_conversion!(SetTarget, true);
