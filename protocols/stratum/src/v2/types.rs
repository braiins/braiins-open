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

use std::convert::TryFrom;

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

macro_rules! sized_string_type {
    ($name:ident, $max_len:expr) => {
        #[derive(PartialEq, Eq, Serialize, Deserialize, Default, Clone, Debug)]
        pub struct $name(std::string::String);

        impl $name {
            const MAX_LEN: usize = $max_len;

            #[inline]
            pub fn new() -> Self {
                Self(std::string::String::new())
            }

            pub fn from_string(s: String) -> Self {
                Self::try_from(s).expect(concat!(
                    "Could not convert String to ",
                    "$name",
                    " - string length out of range."
                ))
            }

            pub fn from_str(s: &str) -> Self {
                Self::try_from(s).expect(concat!(
                    "Could not convert &'str to ",
                    "$name",
                    " - string length out of range."
                ))
            }

            #[inline]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            #[inline]
            pub fn as_bytes(&self) -> &[u8] {
                self.0.as_bytes()
            }
        }

        impl std::convert::TryFrom<std::string::String> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: std::string::String) -> std::result::Result<Self, ()> {
                if (1..=Self::MAX_LEN).contains(&s.len()) {
                    Ok(Self(s))
                } else {
                    Err(())
                }
            }
        }

        impl<'a> std::convert::TryFrom<&'a str> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: &'a str) -> std::result::Result<Self, ()> {
                if (1..=Self::MAX_LEN).contains(&s.len()) {
                    Ok(Self(String::from(s)))
                } else {
                    Err(())
                }
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                self.0.as_str()
            }
        }

        impl AsRef<[u8]> for $name {
            #[inline]
            fn as_ref(&self) -> &[u8] {
                self.0.as_bytes()
            }
        }

        impl From<$name> for std::string::String {
            #[inline]
            fn from(s: $name) -> std::string::String {
                s.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                self.0.as_str()
            }
        }
    };
}

sized_string_type!(String31, 31);
sized_string_type!(String255, 255);
