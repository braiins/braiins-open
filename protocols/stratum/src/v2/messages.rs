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

use std::convert::TryFrom;
use std::io::{Cursor, Write};

use packed_struct::PackedStruct;
use serde;
use serde::{Deserialize, Serialize};

use super::framing::{Header, MessageType};
use super::types::*;
use crate::error::{Error, Result};

#[cfg(not(feature = "v2json"))]
use super::serialization;
#[cfg(feature = "v2json")]
use serde_json as serialization;

#[cfg(test)]
mod test;

/// Serializes the specified message into a frame
fn serialize_with_header<M: Serialize>(
    message: M,
    msg_type: MessageType,
) -> Result<ii_wire::TxFrame> {
    let buffer = Vec::with_capacity(128); // This is what serde does

    // TODO review the behavior below, that would mean it would optimize the move completely?
    // Cursor is used here to write the serialized message and then the header in front of it
    // otherwise the the serialized message would have to be shifted in memory
    let mut cursor = Cursor::new(buffer);
    cursor.set_position(Header::SIZE as u64);
    serialization::to_writer(&mut cursor, &message)?;

    let payload_len = cursor.position() as usize - Header::SIZE;
    let header = Header::new(msg_type, payload_len);
    cursor.set_position(0);
    cursor.write(&header.pack())?;

    Ok(ii_wire::Frame::new(cursor.into_inner().into_boxed_slice()))
}

macro_rules! impl_conversion {
    ($message:ident, /*$msg_type:path,*/ $handler_fn:ident) => {
        impl TryFrom<$message> for ii_wire::TxFrame {
            type Error = Error;

            fn try_from(m: $message) -> Result<ii_wire::TxFrame> {
                serialize_with_header(&m, MessageType::$message)
            }
        }

        // TODO: the from type should be RxFrame (?)
        impl TryFrom<&[u8]> for $message {
            type Error = Error;

            fn try_from(msg: &[u8]) -> Result<Self> {
                serialization::from_slice(msg).map_err(Into::into)
            }
        }

        //  specific protocol implementation
        impl ii_wire::Payload<super::Protocol> for $message {
            fn accept(
                &self,
                msg: &ii_wire::Message<super::Protocol>,
                handler: &mut <super::Protocol as ii_wire::Protocol>::Handler,
            ) {
                handler.$handler_fn(msg, self);
            }
        }
    };
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct SetupMiningConnection {
    pub protocol_version: u16,
    pub connection_url: String255,
    /// for header only mining, this fields stays at 0
    pub required_extranonce_size: u16,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct SetupMiningConnectionSuccess {
    pub used_protocol_version: u16,
    pub max_extranonce_size: u16,
    pub pub_key: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetupMiningConnectionError {
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenChannel {
    pub req_id: u32,
    pub user: String,
    pub extended: bool,
    pub device: DeviceInfo,
    pub nominal_hashrate: f32,
    pub max_target_nbits: u32,
    pub aggregated_device_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenChannelSuccess {
    pub req_id: u32,
    pub channel_id: u32,
    /// Optional device ID provided by the upstream if none was sent as part of DeviceInfo
    pub dev_id: Option<String>,
    /// Initial target for mining
    pub init_target: Uint256Bytes,
    /// See SetGroupChannel for details
    pub group_channel_id: u32,
    // TODO specify signature type
    // pub signature:???
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenChannelError {
    pub req_id: u32,
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateChannel;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateChannelError;

pub struct CloseChannel;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitShares {
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
    pub new_shares_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitSharesError {
    pub channel_id: u32,
    pub seq_num: u32,
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NewMiningJob {
    pub channel_id: u32,
    pub job_id: u32,
    pub block_height: u32,
    pub merkle_root: Uint256Bytes,
    pub version: u32,
}

pub struct NewExtendedMiningJob;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetNewPrevHash {
    pub block_height: u32,
    pub prev_hash: Uint256Bytes,
    pub min_ntime: u32,
    pub max_ntime_offset: u16,
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

impl_conversion!(SetupMiningConnection, visit_setup_mining_connection);
impl_conversion!(
    SetupMiningConnectionSuccess,
    visit_setup_mining_connection_success
);
impl_conversion!(
    SetupMiningConnectionError,
    visit_setup_mining_connection_error
);
impl_conversion!(OpenChannel, visit_open_channel);
impl_conversion!(OpenChannelSuccess, visit_open_channel_success);
impl_conversion!(OpenChannelError, visit_open_channel_error);
impl_conversion!(UpdateChannel, visit_update_channel);
impl_conversion!(UpdateChannelError, visit_update_channel_error);
impl_conversion!(SubmitShares, visit_submit_shares);
impl_conversion!(SubmitSharesSuccess, visit_submit_shares_success);
impl_conversion!(SubmitSharesError, visit_submit_shares_error);
impl_conversion!(NewMiningJob, visit_new_mining_job);
impl_conversion!(SetNewPrevHash, visit_set_new_prev_hash);
impl_conversion!(SetTarget, visit_set_target);
