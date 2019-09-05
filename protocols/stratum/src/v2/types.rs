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

//! This module provides custom types used in Stratum V2 messages

use serde;
use serde::{Deserialize, Serialize};

/// Device specific information - all parts are optional and could be empty strings
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub vendor: String,
    pub hw_rev: String,
    pub fw_ver: String,
    pub dev_id: String,
}

// TODO consolidate the u8;32 copied all over the place into an alias
//type Uint256Inner = [u8; 32];

/// Custom type for serializing the sha256 values
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
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

impl Into<uint::U256> for Uint256Bytes {
    fn into(self) -> uint::U256 {
        uint::U256::from_little_endian(&self.0)
    }
}

impl From<uint::U256> for Uint256Bytes {
    fn from(value: uint::U256) -> Self {
        let mut bytes = Uint256Bytes([0; 32]);
        value.to_little_endian(bytes.as_mut());
        bytes
    }
}

impl From<Uint256Bytes> for ii_bitcoin::Target {
    fn from(value: Uint256Bytes) -> Self {
        value.0.into()
    }
}
