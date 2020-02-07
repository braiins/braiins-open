// Copyright (C) 2020  Braiins Systems s.r.o.
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

//! Common module that provides helper functionality to handle payload of protocol frames

use bytes::{buf::BufMutExt, BytesMut};
use std::fmt;

use ii_async_compat::bytes;

use crate::error::{Result, ResultExt};

/// This trait allows lazy serialization of a frame payload
pub trait SerializablePayload: Send + Sync {
    /// The payload is serialized to a specified `writer`
    fn serialize_to_writer(&self, writer: &mut dyn std::io::Write) -> Result<()>;
}

/// Frame payload that either consists of a series of bytes or a dynamic payload that can be
/// serialized on demand.
///
/// NOTE: The dynamic payload is currently not intentionally cached when being physically
/// serialized via `Payload::to_bytes_mut()` as the case of repeatedly serializing the same frame is
/// rather rare.
/// Should this become a performance issue, we can wrap it into `once_cell::{un,}sync::Lazy`.
pub enum Payload {
    SerializedBytes(BytesMut),
    LazyBytes(Box<dyn SerializablePayload>),
}

impl Payload {
    /// Helper associated method that converts `serializable_payload` to `BytesMut`
    fn serializable_payload_to_bytes_mut(
        payload: &Box<dyn SerializablePayload>,
    ) -> Result<BytesMut> {
        // TODO: use some default capacity
        let payload_bytes = BytesMut::new();
        let mut writer = payload_bytes.writer();
        payload.serialize_to_writer(&mut writer)?;
        Ok(writer.into_inner())
    }

    /// Checks whether the payload contains already a deserialized object.
    pub fn is_deserialized_message(&self) -> bool {
        match self {
            Self::LazyBytes(_) => true,
            Self::SerializedBytes(_) => false,
        }
    }

    /// Consumes the payload and provides the serializable inner variant of the payload or None
    pub fn into_message(self) -> Option<Box<dyn SerializablePayload<P>>> {
        match self {
            Self::SerializedBytes(_) => None,
            Self::LazyBytes(payload) => Some(payload),
        }
    }
    /// Consumes the payload and transforms it into a `BytesMut`
    /// TODO: consider returning a read-only buffer
    pub fn into_bytes_mut(self) -> Result<BytesMut> {
        match self {
            Self::SerializedBytes(payload) => Ok(payload),
            Self::LazyBytes(payload) => Self::serializable_payload_to_bytes_mut(&payload),
        }
    }

    /// Expensive variant for converting the payload into `BytesMut`. It creates a copy of the
    /// payload should the frame be already in serialized variant. The reason why we cannot use
    /// `into_bytes_mut(self.clone())` is that the LazyBytes variant is not easily clonable as it
    /// is a trait object. We would have to provide a custom clone method for it.
    /// TODO: consider returning a read-only buffer
    pub fn to_bytes_mut(&self) -> Result<BytesMut> {
        match self {
            Self::SerializedBytes(ref payload) => Ok(payload.clone()),
            Self::LazyBytes(ref payload) => Self::serializable_payload_to_bytes_mut(payload),
        }
    }

    /// Serializes the payload directly into the `writer` without creating any intermediate buffers
    pub fn serialize_to_writer<T: std::io::Write>(&self, writer: &mut T) -> Result<()> {
        match &self {
            Self::SerializedBytes(payload) => writer
                .write(payload)
                .context("Serialize static payload")
                .map(|_| ())
                .map_err(Into::into),
            Self::LazyBytes(payload) => payload
                .serialize_to_writer(writer)
                .context("Serialize dynamic payload")
                .map_err(Into::into),
        }
    }
}

/// Comparing 2 payloads is expensive as it results converting the payload into a BytesMut to
/// cover both variants of the Payload. The advantage of this uniform approach is that we can
/// compare Payloads created under different circumstances (`SerializedBytes` or `LazyBytes`
/// variants).
impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        // We have to successfully unwrap both conversion results before proceeding with comparison.
        // Any error results in indicating a mismatch
        if let Ok(self_bytes) = self.to_bytes_mut() {
            if let Ok(other_bytes) = other.to_bytes_mut() {
                self_bytes == other_bytes
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl From<BytesMut> for Payload {
    fn from(payload: BytesMut) -> Self {
        Self::SerializedBytes(payload)
    }
}

impl fmt::Debug for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SerializedBytes(payload) => write!(f, "S{:x?}", payload.to_vec()),
            Self::LazyBytes(_) => write!(f, "L{:x?}", self.to_bytes_mut().map_err(|_| fmt::Error)?),
        }
    }
}
