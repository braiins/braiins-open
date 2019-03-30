//! All stratum V2 protocol messages

use super::types::*;
use serde;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SetupMiningConnection {
    pub protocol_version: u16,
    pub connection_url: String,
    /// for header only mining, this fields stays at 0
    pub required_extranonce_size: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct SetupMiningConnectionSuccess {
    pub used_protocol_version: u16,
    pub max_extranonce_size: u16,
    pub pub_key: Vec<u8>,
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
