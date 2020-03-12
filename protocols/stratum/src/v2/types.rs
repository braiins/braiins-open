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
use std::fmt::{self, Debug};
use std::ops::Deref;

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

impl From<ii_bitcoin::Target> for Uint256Bytes {
    fn from(target: ii_bitcoin::Target) -> Self {
        target.into_inner().into()
    }
}

macro_rules! sized_string_type {
    ($name:ident, $min_len:expr, $max_len:expr) => {
        #[derive(PartialEq, Eq, Serialize, Deserialize, Default, Clone, Debug)]
        pub struct $name(String);

        impl $name {
            const MIN_LEN: usize = $min_len;
            const MAX_LEN: usize = $max_len;

            #[inline]
            pub fn new() -> Self {
                Self::default()
            }

            pub fn from_string(s: String) -> Self {
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

            pub fn to_string(&self) -> String {
                String::from(&*self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: String) -> Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&s.len()) {
                    Ok(Self(s))
                } else {
                    Err(())
                }
            }
        }

        impl<'a> TryFrom<&'a str> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: &'a str) -> Result<Self, ()> {
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

        impl From<$name> for String {
            #[inline]
            fn from(s: $name) -> String {
                s.0
            }
        }

        impl Deref for $name {
            type Target = String;

            fn deref(&self) -> &String {
                &self.0
            }
        }
    };
}

macro_rules! sized_bytes_type {
    ($name:ident, $min_len:expr, $max_len:expr) => {
        #[derive(PartialEq, Eq, Serialize, Deserialize, Default, Clone, Debug)]
        pub struct $name(Box<[u8]>);

        impl $name {
            const MIN_LEN: usize = $min_len;
            const MAX_LEN: usize = $max_len;

            #[inline]
            pub fn new() -> Self {
                Self::default()
            }

            pub fn from_vec(v: Vec<u8>) -> Self {
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

        impl TryFrom<Vec<u8>> for $name {
            type Error = ();

            #[inline]
            fn try_from(v: Vec<u8>) -> Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&v.len()) {
                    Ok(Self(v.into_boxed_slice()))
                } else {
                    Err(())
                }
            }
        }

        impl<'a> TryFrom<&'a [u8]> for $name {
            type Error = ();

            #[inline]
            fn try_from(s: &'a [u8]) -> Result<Self, ()> {
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

        impl From<$name> for Vec<u8> {
            #[inline]
            fn from(s: $name) -> Vec<u8> {
                s.0.into_vec()
            }
        }

        impl From<$name> for Box<[u8]> {
            #[inline]
            fn from(s: $name) -> Box<[u8]> {
                s.0
            }
        }

        impl Deref for $name {
            type Target = [u8];

            fn deref(&self) -> &[u8] {
                &*self.0
            }
        }
    };
}

macro_rules! sized_seq_type {
    ($name:ident, $min_len:expr, $max_len:expr) => {
        #[derive(Serialize, Deserialize)]
        pub struct $name<T>(
            #[serde(bound(deserialize = "T: Serialize + for<'dx> Deserialize<'dx>"))] Vec<T>,
        )
        where
            T: Serialize + for<'dx> Deserialize<'dx>;

        impl<T> $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx>,
        {
            const MIN_LEN: usize = $min_len;
            const MAX_LEN: usize = $max_len;

            #[inline]
            pub fn new() -> Self {
                Self(vec![])
            }

            pub fn from_vec(v: Vec<T>) -> Self {
                Self::try_from(v).expect(concat!(
                    "Could not convert Vec to ",
                    stringify!($name),
                    " - Vec length out of range."
                ))
            }
        }

        impl<T> $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx> + Clone,
        {
            pub fn from_slice(s: &[T]) -> Self {
                Self::try_from(s).expect(concat!(
                    "Could not convert &[u8] to ",
                    stringify!($name),
                    " - slice length out of range."
                ))
            }
        }

        impl<T> TryFrom<Vec<T>> for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx>,
        {
            type Error = ();

            #[inline]
            fn try_from(vec: Vec<T>) -> Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&vec.len()) {
                    Ok(Self(vec))
                } else {
                    Err(())
                }
            }
        }

        impl<'a, T> TryFrom<&'a [T]> for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx> + Clone,
        {
            type Error = ();

            #[inline]
            fn try_from(s: &'a [T]) -> Result<Self, ()> {
                if (Self::MIN_LEN..=Self::MAX_LEN).contains(&s.len()) {
                    Ok(Self(s.into()))
                } else {
                    Err(())
                }
            }
        }

        impl<T> AsRef<[T]> for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx>,
        {
            #[inline]
            fn as_ref(&self) -> &[T] {
                &*self.0
            }
        }

        // This should really be a From impl,
        // but this can't be done with rustc 1.40 due to coherence rules.
        // FIXME: this once bosminer/stratum upgrade to rustc 1.41
        impl<T> Into<Vec<T>> for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx>,
        {
            fn into(self) -> Vec<T> {
                self.0
            }
        }

        // FIXME: dtto
        impl<T> Into<Box<[T]>> for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx>,
        {
            fn into(self) -> Box<[T]> {
                self.0.into_boxed_slice()
            }
        }

        impl<T> Deref for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx>,
        {
            type Target = [T];

            fn deref(&self) -> &[T] {
                &*self.0
            }
        }

        impl<T> PartialEq for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx> + PartialEq,
        {
            fn eq(&self, other: &Self) -> bool {
                self.0.eq(&other.0)
            }
        }

        impl<T> Eq for $name<T> where T: Serialize + for<'dx> Deserialize<'dx> + PartialEq {}

        impl<T> Debug for $name<T>
        where
            T: Serialize + for<'dx> Deserialize<'dx> + Debug,
        {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
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

sized_seq_type!(Seq0_255, 0, 255);
sized_seq_type!(Seq0_64k, 0, 65535);

/// Device specific information - all parts are optional and could be empty strings
/// TODO: Fix minimal string length in the Stratum V2 specification
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub vendor: Str0_255,
    pub hw_rev: Str0_255,
    pub fw_ver: Str0_255,
    pub dev_id: Str0_255,
}

/// PubKey for authenticating some protocol messages
/// TODO: Preliminary as exact signing algorithm has not been chosen, we may even have this as
/// dynamic field Bytes0_255
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PubKey([u8; 0]);

impl PubKey {
    pub fn new() -> Self {
        PubKey([0; 0])
    }
}
