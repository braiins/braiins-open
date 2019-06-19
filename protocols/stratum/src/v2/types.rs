//! This module provides custom types used in Stratum V2 messages

use serde;
use serde::{Deserialize, Serialize};

/// Device specific information - all parts are optional and could be empty strings
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceInfo {
    pub vendor: String,
    pub hw_rev: String,
    pub fw_ver: String,
    pub dev_id: String,
}

/// Custom type for serializing the sha256 values
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Uint256Bytes(pub [u8; 32]);

// TODO review whether Deref might be suitable
impl AsRef<[u8; 32]> for Uint256Bytes {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl AsMut<[u8; 32]> for Uint256Bytes {
    fn as_mut(&mut self) -> &mut [u8; 32] {
        &mut self.0
    }
}
