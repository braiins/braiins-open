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

pub use std::convert::{TryFrom, TryInto};

use serde;
use serde::{Deserialize, Serialize};

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
    ($name:ident, $min_len:expr, $max_len:expr) => {
        #[derive(PartialEq, Eq, Serialize, Deserialize, Default, Clone, Debug)]
        pub struct $name(std::string::String);

        impl $name {
            const MIN_LEN: usize = $min_len;
            const MAX_LEN: usize = $max_len;

            #[inline]
            pub fn new() -> Self {
                Self::default()
            }

            pub fn from_string(s: std::string::String) -> Self {
                Self::try_from(s).expect(concat!(
                    "Could not convert String to ",
                    stringify!($name),
                    " - string length out of range."
                ))
            }

            pub fn from_str(s: &str) -> Self {
                Self::try_from(s).expect(concat!(
                    "Could not convert &'str to ",
                    stringify!($name),
                    " - string length out of range."
                ))
            }

            pub fn to_string(&self) -> std::string::String {
                std::string::String::from(&*self.0)
            }
        }

        impl std::convert::TryFrom<std::string::String> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: std::string::String) -> std::result::Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&s.len()) {
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
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&s.len()) {
                    Ok(Self(s.into()))
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
            type Target = std::string::String;

            fn deref(&self) -> &std::string::String {
                &self.0
            }
        }
    };
}

macro_rules! sized_bytes_type {
    ($name:ident, $min_len:expr, $max_len:expr) => {
        #[derive(PartialEq, Eq, Serialize, Deserialize, Default, Clone, Debug)]
        pub struct $name(std::boxed::Box<[u8]>);

        impl $name {
            const MIN_LEN: usize = $min_len;
            const MAX_LEN: usize = $max_len;

            #[inline]
            pub fn new() -> Self {
                Self::default()
            }

            pub fn from_vec(v: std::vec::Vec<u8>) -> Self {
                Self::try_from(v).expect(concat!(
                    "Could not convert Vec to ",
                    stringify!($name),
                    " - Vec length out of range."
                ))
            }

            pub fn from_slice(s: &[u8]) -> Self {
                Self::try_from(s).expect(concat!(
                    "Could not convert &[u8] to ",
                    stringify!($name),
                    " - slice length out of range."
                ))
            }
        }

        impl std::convert::TryFrom<std::vec::Vec<u8>> for $name {
            type Error = ();

            #[inline]
            fn try_from(v: std::vec::Vec<u8>) -> std::result::Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&v.len()) {
                    Ok(Self(v.into_boxed_slice()))
                } else {
                    Err(())
                }
            }
        }

        impl<'a> std::convert::TryFrom<&'a [u8]> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: &'a [u8]) -> std::result::Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&s.len()) {
                    Ok(Self(s.into()))
                } else {
                    Err(())
                }
            }
        }

        impl AsRef<[u8]> for $name {
            #[inline]
            fn as_ref(&self) -> &[u8] {
                &*self.0
            }
        }

        impl From<$name> for std::vec::Vec<u8> {
            #[inline]
            fn from(s: $name) -> std::vec::Vec<u8> {
                s.0.into_vec()
            }
        }

        impl From<$name> for std::boxed::Box<[u8]> {
            #[inline]
            fn from(s: $name) -> std::boxed::Box<[u8]> {
                s.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = [u8];

            fn deref(&self) -> &[u8] {
                &*self.0
            }
        }
    };
}

sized_string_type!(Str0_32, 0, 32);
sized_string_type!(Str1_32, 1, 32);
sized_string_type!(Str0_255, 0, 255);
sized_string_type!(Str1_255, 1, 255);

sized_bytes_type!(Bytes0_32, 0, 32);
sized_bytes_type!(Bytes1_32, 1, 32);
sized_bytes_type!(Bytes0_255, 0, 255);
sized_bytes_type!(Bytes1_255, 1, 255);
sized_bytes_type!(Bytes0_64k, 0, 65535);
sized_bytes_type!(Bytes1_64k, 1, 65535);

/// Device specific information - all parts are optional and could be empty strings
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub vendor: Str1_255,
    pub hw_rev: Str1_255,
    pub fw_ver: Str1_255,
    pub dev_id: Str0_255,
}
