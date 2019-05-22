//! All stratum V2 protocol messages

use super::framing::{Header, MessageType};
use super::types::*;
use packed_struct::PackedStruct;
use serde;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::io::{Cursor, Write};
use wire;

#[cfg(test)]
pub mod test;

/// Serializes the specified message into a frame
/// TODO maybe this should return an error, too
fn serialize_with_header<M: Serialize>(message: M, msg_type: MessageType) -> wire::TxFrame {
    // FIXME: temporary JSON serialization

    let buffer = Vec::with_capacity(128); // This is what serde does

    // TODO review the behavior below, that would mean it would optimize the move completely?
    // Cursor is used here to write JSON and then the header in front of it
    // otherwise the JSON would have to be shifted in memory
    let mut cursor = Cursor::new(buffer);
    cursor.set_position(Header::SIZE as u64);
    serde_json::to_writer(&mut cursor, &message).expect("Error serializing JSON value"); // This shouldn't actually fail

    let payload_len = cursor.position() as usize - Header::SIZE;
    let header = Header::new(msg_type, payload_len);
    cursor.set_position(0);
    // TODO this may also fail...
    cursor.write(&header.pack()).expect("Writing header failed");

    wire::Frame::new(cursor.into_inner().into_boxed_slice())
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct SetupMiningConnection {
    pub protocol_version: u16,
    pub connection_url: String,
    /// for header only mining, this fields stays at 0
    pub required_extranonce_size: u16,
}

/// TODO this code is to be generated
impl From<SetupMiningConnection> for wire::TxFrame {
    fn from(m: SetupMiningConnection) -> wire::TxFrame {
        serialize_with_header(&m, MessageType::SetupMiningConnection)
    }
}

/// TODO: the from type should be RxFrame
impl TryFrom<&[u8]> for SetupMiningConnection {
    type Error = crate::error::Error;

    fn try_from(msg: &[u8]) -> Result<Self, Self::Error> {
        serde_json::from_slice(msg).map_err(Into::into)
    }
}

//  specific protocol implementation
impl wire::Payload<super::V2Protocol> for SetupMiningConnection {
    fn accept(
        &self,
        msg: &wire::Message<super::V2Protocol>,
        handler: &<super::V2Protocol as wire::ProtocolBase>::Handler,
    ) {
        handler.visit_setup_mining_connection(msg, self);
    }
}

// specific protocol implementation
//impl wire::Payload<P> for SetupMiningConnection {
//    fn accept(&self, msg: &wire::Message<P>, handler: &<P as wire::Protocol>::Handler) {
//        unimplemented!()
//    }
//}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct SetupMiningConnectionSuccess {
    pub used_protocol_version: u16,
    pub max_extranonce_size: u16,
    pub pub_key: Vec<u8>,
}

impl From<SetupMiningConnectionSuccess> for wire::TxFrame {
    fn from(m: SetupMiningConnectionSuccess) -> wire::TxFrame {
        serialize_with_header(&m, MessageType::SetupMiningConnectionSuccess)
    }
}

impl TryFrom<&[u8]> for SetupMiningConnectionSuccess {
    type Error = crate::error::Error;

    fn try_from(msg: &[u8]) -> Result<Self, Self::Error> {
        serde_json::from_slice(msg).map_err(Into::into)
    }
}

//  specific protocol implementation
impl wire::Payload<super::V2Protocol> for SetupMiningConnectionSuccess {
    fn accept(
        &self,
        msg: &wire::Message<super::V2Protocol>,
        handler: &<super::V2Protocol as wire::ProtocolBase>::Handler,
    ) {
        handler.visit_setup_mining_connection_success(msg, self);
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct SetupMiningConnectionError {
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct OpenChannel {
    pub req_id: u32,
    pub user: String,
    pub extended: bool,
    pub device: DeviceInfo,
    pub nominal_hashrate: f32,
    pub max_target_nbits: u32,
    pub aggregated_device_count: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct OpenChannelSuccess {
    pub req_id: u32,
    pub channel_id: u32,
    /// Optional device ID provided by the upstream if none was sent as part of DeviceInfo
    pub dev_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OpenChannelError {
    pub req_id: u32,
    pub code: String,
}

struct UpdateChannel;
struct UpdateChannelError;

struct CloseChannel;

#[derive(Serialize, Deserialize, Debug)]
struct SubmitShares {
    pub channel_id: u32,
    pub seq_num: u32,
    pub job_id: u32,

    pub nonce: u32,
    pub ntime_offset: u16,
    pub version: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct SubmitSharesSuccess {
    pub channel_id: u32,
    pub last_seq_num: u32,
    pub new_submits_accepted_count: u32,
    pub new_shares_count: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct SubmitSharesError {
    pub channel_id: u32,
    pub seq_num: u32,
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct NewMiningJob {
    pub channel_id: u32,
    pub job_id: u32,
    pub block_height: u32,
    pub merkle_root: Uint256Bytes,
}

struct NewExtendedMiningJob;

#[derive(Serialize, Deserialize, Debug)]
struct SetNewPrevhash {
    pub block_height: u32,
    pub prev_hash: Uint256Bytes,
    pub min_ntime: u32,
    pub max_ntime_offset: u16,
    pub nbits: u32,
    // TODO specify signature type
    //pub signature: ??,
}

struct SetCustomMiningJob;
struct SetCustomMiningJobSuccess;
struct Reconnect;

#[derive(Serialize, Deserialize, Debug)]
struct SetTarget {
    pub channel_id: u32,
    pub max_target: Uint256Bytes,
}

struct SetGroupChannel;
