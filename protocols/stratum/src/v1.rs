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

pub mod error;
pub mod framing;
pub mod messages;
pub mod rpc;

use self::error::Error;
pub use self::framing::codec::Codec;
pub use self::framing::{Frame, Framing};
use crate::error::Result;

use bitcoin_hashes::hex::{FromHex, ToHex};
use byteorder::{BigEndian, ByteOrder, LittleEndian, WriteBytesExt};
use hex::FromHexError;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::mem::size_of;
use tokio::net::TcpStream;

const ERROR_ATTRIBUTE: &'static str = "error";
const RESULT_ATTRIBUTE: &'static str = "result";

//// Tcp stream that produces/consumes V2 frames
pub type Framed = tokio_util::codec::Framed<TcpStream, <Framing as ii_wire::Framing>::Codec>;

// Message Id is used for pairing request/response messages
/// TODO spread this type across the protocol and eliminate the `Option<u32>`
pub type MessageId = Option<u32>;

pub struct Protocol;
impl crate::Protocol for Protocol {
    /// Simplified protocol 'header' really contains only an optional message ID
    type Header = MessageId;
}

/// Extranonce 1 introduced as new type to provide shared conversions to/from string
/// TODO: find out correct byte order for extra nonce 1
/// TODO: implement deref trait consolidate use of extra nonce 1
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExtraNonce1(pub HexBytes);

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct HexBytes(Vec<u8>);

impl HexBytes {
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<Vec<u8>> for HexBytes {
    fn as_ref(&self) -> &Vec<u8> {
        &self.0
    }
}

/// fix for error on odd-length hex sequences
/// FIXME: find a nicer solution
fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, FromHexError> {
    if s.len() % 2 != 0 {
        hex::decode(&format!("0{}", s))
    } else {
        hex::decode(s)
    }
}

impl TryFrom<&str> for HexBytes {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        Ok(HexBytes(hex_decode(value)?))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for HexBytes {
    fn from(value: String) -> Self {
        HexBytes::try_from(value.as_str()).unwrap_or(HexBytes(vec![]))
    }
}

/// Helper Serializer
impl Into<String> for HexBytes {
    fn into(self) -> String {
        hex::encode(self.0)
    }
}

/// PrevHash in Stratum V1 has brain-damaged serialization as it swaps bytes of very u32 word
/// into big endian. Therefore, we need a special type for it
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct PrevHash(Vec<u8>);

impl PrevHash {
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Referencing the internal part of the prev hash
impl AsRef<Vec<u8>> for PrevHash {
    fn as_ref(&self) -> &Vec<u8> {
        &self.0
    }
}

/// TODO: implement unit test
impl TryFrom<&str> for PrevHash {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        // Reorder prevhash will be stored via this cursor
        let mut prev_hash_cursor = std::io::Cursor::new(Vec::new());

        // Decode the plain byte array and sanity check
        let prev_hash_stratum_order = hex_decode(value)?;
        if prev_hash_stratum_order.len() != 32 {
            return Err(Error::Json(format!(
                "Incorrect prev hash length: {}",
                prev_hash_stratum_order.len()
            ))
            .into());
        }
        // Swap every u32 from big endian to little endian byte order
        for chunk in prev_hash_stratum_order.chunks(size_of::<u32>()) {
            let prev_hash_word = BigEndian::read_u32(chunk);
            prev_hash_cursor
                .write_u32::<LittleEndian>(prev_hash_word)
                .expect("Internal error: Could not write buffer");
        }

        Ok(PrevHash(prev_hash_cursor.into_inner()))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for PrevHash {
    fn from(value: String) -> Self {
        PrevHash::try_from(value.as_str()).unwrap_or(PrevHash(vec![]))
    }
}

/// Helper Serializer that peforms the reverse process of converting the prev hash into stratum V1
/// ordering
/// TODO: implement unit test
impl Into<String> for PrevHash {
    fn into(self) -> String {
        let mut prev_hash_stratum_cursor = std::io::Cursor::new(Vec::new());
        // swap every u32 from little endian to big endian
        for chunk in self.0.chunks(size_of::<u32>()) {
            let prev_hash_word = LittleEndian::read_u32(chunk);
            prev_hash_stratum_cursor
                .write_u32::<BigEndian>(prev_hash_word)
                .expect("Internal error: Could not write buffer");
        }
        hex::encode(prev_hash_stratum_cursor.into_inner())
    }
}

/// Little-endian hex encoded u32
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct HexU32Le(pub u32);

impl TryFrom<&str> for HexU32Le {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        let parsed_bytes: [u8; 4] = FromHex::from_hex(value.trim_start_matches("0x"))?;
        Ok(HexU32Le(u32::from_le_bytes(parsed_bytes)))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for HexU32Le {
    fn from(value: String) -> Self {
        HexU32Le::try_from(value.as_str()).unwrap_or(HexU32Le(0))
    }
}

/// Helper Serializer
impl Into<String> for HexU32Le {
    fn into(self) -> String {
        self.0.to_le_bytes().to_hex()
    }
}

/// Big-endian alternative of the HexU32
/// TODO: find out how to consolidate/parametrize it with generic parameters
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct HexU32Be(pub u32);

impl TryFrom<&str> for HexU32Be {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        let parsed_bytes: [u8; 4] = FromHex::from_hex(value.trim_start_matches("0x"))?;
        Ok(HexU32Be(u32::from_be_bytes(parsed_bytes)))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for HexU32Be {
    fn from(value: String) -> Self {
        HexU32Be::try_from(value.as_str()).unwrap_or(HexU32Be(0))
    }
}

/// Helper Serializer
impl Into<String> for HexU32Be {
    fn into(self) -> String {
        self.0.to_be_bytes().to_hex()
    }
}

#[cfg(test)]
mod test;
