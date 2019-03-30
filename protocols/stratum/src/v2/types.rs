//! This module provides custom types used in Stratum V2 messages

use serde;
use serde::{Deserialize, Serialize};

/// Device specific information - all parts are optional and could be empty strings
#[derive(Serialize, Deserialize, Debug)]
pub struct DeviceInfo {
    pub vendor: String,
    pub hw_rev: String,
    pub fw_ver: String,
    pub dev_id: String,
}

/// Custom type for serializing the sha256 values
#[derive(Serialize, Deserialize, Debug)]
pub struct Uint256Bytes([u8; 32]);
