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

use async_trait::async_trait;
use packed_struct_codegen::PrimitiveEnum_u8;
use serde;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use super::framing;
#[cfg(not(feature = "v2json"))]
use super::serialization;
use super::types::*;
use super::Protocol;
use crate::error::{Error, Result};
use crate::AnyPayload;
#[cfg(feature = "v2json")]
use serde_json as serialization;

#[cfg(test)]
mod test;

/// Generates conversion for base protocol (extension 0)
macro_rules! impl_base_conversion {
    ($message:tt, $is_channel_msg:tt, $handler_fn:tt) => {
        impl_conversion!(0, $message, $is_channel_msg, $handler_fn);
    };
}

/// Generates conversion for a specified extension message
macro_rules! impl_conversion {
    ($extension_id:tt, $message:tt, $is_channel_msg:expr, $handler_fn:tt) => {
        // NOTE: $message and $handler_fn need to be tt because of https://github.com/dtolnay/async-trait/issues/46

        impl TryFrom<$message> for framing::Frame {
            type Error = Error;

            /// Prepares a frame for serializing the specified message just in time (the message
            /// is treated as a `SerializablePayload`)
            fn try_from(m: $message) -> Result<Self> {
                Ok(framing::Frame::from_serializable_payload(
                    $is_channel_msg,
                    $extension_id,
                    MessageType::$message as framing::MsgType,
                    m,
                ))
            }
        }

        impl TryFrom<&[u8]> for $message {
            type Error = Error;

            fn try_from(msg: &[u8]) -> Result<Self> {
                serialization::from_slice(msg).map_err(Into::into)
            }
        }

        impl TryFrom<framing::Frame> for $message {
            type Error = Error;

            fn try_from(frame: framing::Frame) -> Result<Self> {
                let (_header, payload) = frame.split();
                let payload = payload.into_bytes_mut()?;
                Self::try_from(&payload[..])
            }
        }

        /// Each message is a `AnyPayload/SerializablePayload` object that can be serialized into
        /// `writer`
        #[async_trait]
        impl AnyPayload<Protocol> for $message {
            async fn accept(
                &self,
                header: &<Protocol as crate::Protocol>::Header,
                handler: &mut <Protocol as crate::Protocol>::Handler,
            ) {
                handler.$handler_fn(header, self).await;
            }

            fn serialize_to_writer(&self, writer: &mut dyn std::io::Write) -> Result<()> {
                serialization::to_writer(writer, self).map_err(Into::into)
            }
        }
    };
}

/// All message recognized by the protocol
#[derive(PrimitiveEnum_u8, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageType {
    SetupConnection = 0x00,
    SetupConnectionSuccess = 0x01,
    SetupConnectionError = 0x02,
    ChannelEndpointChanged = 0x03,

    // Mining Protocol
    OpenStandardMiningChannel = 0x10,
    OpenStandardMiningChannelSuccess = 0x11,
    OpenStandardMiningChannelError = 0x12,
    OpenExtendedMiningChannel = 0x13,
    OpenExtendedMiningChannelSuccess = 0x14,
    OpenExtendedMiningChannelError = 0x15,
    UpdateChannel = 0x16,
    UpdateChannelError = 0x17,
    CloseChannel = 0x18,
    SetExtranoncePrefix = 0x19,
    SubmitSharesStandard = 0x1a,
    SubmitSharesExtended = 0x1b,
    SubmitSharesSuccess = 0x1c,
    SubmitSharesError = 0x1d,
    NewMiningJob = 0x1e,
    NewExtendedMiningJob = 0x1f,
    SetNewPrevHash = 0x20,
    SetTarget = 0x21,
    SetCustomMiningJob = 0x22,
    SetCustomMiningJobSuccess = 0x23,
    SetCustomMiningError = 0x24,
    Reconnect = 0x25,
    SetGroupChannel = 0x26,
}

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupConnectionSuccess {
    pub used_version: u16,
    /// TODO: specify an enum for flags
    pub flags: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetupConnectionError {
    pub flags: u32,
    pub code: Str0_255,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenStandardMiningChannel {
    pub req_id: u32,
    pub user: Str1_255,
    pub nominal_hashrate: f32,
    pub max_target: Uint256Bytes,
}

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenStandardMiningChannelError {
    pub req_id: u32,
    pub code: Str0_32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UpdateChannel;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UpdateChannelError;

pub struct CloseChannel;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesStandard {
    pub channel_id: u32,
    pub seq_num: u32,
    pub job_id: u32,

    pub nonce: u32,
    pub ntime: u32,
    pub version: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesSuccess {
    pub channel_id: u32,
    pub last_seq_num: u32,
    pub new_submits_accepted_count: u32,
    pub new_shares_sum: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesError {
    pub channel_id: u32,
    pub seq_num: u32,
    pub code: Str0_32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewMiningJob {
    pub channel_id: u32,
    pub job_id: u32,
    pub future_job: bool,
    pub version: u32,
    pub merkle_root: Uint256Bytes,
}

pub struct NewExtendedMiningJob;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetNewPrevHash {
    pub channel_id: u32,
    pub job_id: u32,
    pub prev_hash: Uint256Bytes,
    pub min_ntime: u32,
    pub nbits: u32,
    // TODO specify signature type
    //pub signature: ??,
}

pub struct SetCustomMiningJob;
pub struct SetCustomMiningJobSuccess;
pub struct Reconnect;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetTarget {
    pub channel_id: u32,
    pub max_target: Uint256Bytes,
}

pub struct SetGroupChannel;

impl_base_conversion!(SetupConnection, false, visit_setup_connection);
impl_base_conversion!(
    SetupConnectionSuccess,
    false,
    visit_setup_connection_success
);
impl_base_conversion!(SetupConnectionError, false, visit_setup_connection_error);
impl_base_conversion!(
    OpenStandardMiningChannel,
    false,
    visit_open_standard_mining_channel
);
impl_base_conversion!(
    OpenStandardMiningChannelSuccess,
    false,
    visit_open_standard_mining_channel_success
);
impl_base_conversion!(
    OpenStandardMiningChannelError,
    false,
    visit_open_standard_mining_channel_error
);
impl_base_conversion!(UpdateChannel, true, visit_update_channel);
impl_base_conversion!(UpdateChannelError, true, visit_update_channel_error);
impl_base_conversion!(SubmitSharesStandard, true, visit_submit_shares_standard);
impl_base_conversion!(SubmitSharesSuccess, true, visit_submit_shares_success);
impl_base_conversion!(SubmitSharesError, true, visit_submit_shares_error);
impl_base_conversion!(NewMiningJob, true, visit_new_mining_job);
impl_base_conversion!(SetNewPrevHash, true, visit_set_new_prev_hash);
impl_base_conversion!(SetTarget, true, visit_set_target);
