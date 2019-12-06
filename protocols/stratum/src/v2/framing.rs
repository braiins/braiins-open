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

//! This module defines basic framing and all protocol message types

use lazy_static::lazy_static;
use std::collections::HashMap;

use packed_struct::prelude::*;
use packed_struct_codegen::PackedStruct;
use packed_struct_codegen::PrimitiveEnum_u8;

pub mod codec;

/// Header of the protocol message
#[derive(PackedStruct, Debug)]
#[packed_struct(endian = "msb", bit_numbering = "lsb0", size_bytes = "6")]
pub struct Header {
    // WARN: This struct's layout needs to be kept in sync
    // with the consts in the impl block below.
    // This is because the Codec needs to know the offset
    // and size of the msg_length field in the packed byte array
    // (not in the struct in-memory, so we can't auto-deduce this).
    #[packed_field(bits = "47")]
    pub is_channel_message: bool,
    #[packed_field(bits = "46:32")]
    pub extension_type: Integer<u16, packed_bits::Bits15>,
    #[packed_field(bits = "31:24")]
    pub msg_type: u8,
    #[packed_field(bits = "23:0")]
    pub msg_length: Integer<u32, packed_bits::Bits24>,
}

impl Header {
    pub const SIZE: usize = 6;
    pub const LEN_OFFSET: usize = 3;
    pub const LEN_SIZE: usize = 3;

    pub fn new(msg_type: MessageType, msg_length: usize) -> Header {
        assert!(msg_length <= 0xffffff, "Message too large");
        let msg_length = (msg_length as u32).into();

        Header {
            is_channel_message: msg_type.is_channel_message(),
            extension_type: 0.into(),
            msg_type: msg_type.to_primitive(),
            msg_length,
        }
    }

    pub fn unpack_and_swap_endianness(raw_msg: &[u8]) -> Result<Self, packed_struct::PackingError> {
        let mut swapped_raw_msg = [0u8; Self::SIZE];
        swapped_raw_msg.clone_from_slice(&raw_msg);
        swapped_raw_msg.swap(0, 1);
        swapped_raw_msg.swap(3, 5);

        Self::unpack_from_slice(&swapped_raw_msg)
    }

    pub fn pack_and_swap_endianness(&self) -> [u8; Self::SIZE] {
        let mut output: [u8; Self::SIZE] = self.pack();
        output.swap(0, 1);
        output.swap(3, 5);
        output
    }
}

/// All message recognized by the protocol
//#[derive(PrimitiveEnum_u8, Clone, Copy, Debug, PartialEq)]
#[derive(PrimitiveEnum_u8, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MessageType {
    SetupConnection = 0x00,
    SetupConnectionSuccess = 0x01,
    SetupConnectionError = 0x02,
    ChannelEndpointChanged = 0x03,

    // Mining Protocol
    OpenStandardMiningChannel = 0x10,
    OpenStandardMiningChannelSuccess = 0x11,
    OpenStandardMiningChannelError = 0x12,
    OpenExtendedMiningChannel = 0x13,
    OpenExtendedMiningChannelSuccess = 0x14,
    OpenExtendedMiningChannelError = 0x15,
    UpdateChannel = 0x16,
    UpdateChannelError = 0x17,
    CloseChannel = 0x18,
    SetExtranoncePrefix = 0x19,
    SubmitSharesStandard = 0x1a,
    SubmitSharesExtended = 0x1b,
    SubmitSharesSuccess = 0x1c,
    SubmitSharesError = 0x1d,
    NewMiningJob = 0x1e,
    NewExtendedMiningJob = 0x1f,
    SetNewPrevHash = 0x20,
    SetTarget = 0x21,
    SetCustomMiningJob = 0x22,
    SetCustomMiningJobSuccess = 0x23,
    SetCustomMiningError = 0x24,
    Reconnect = 0x25,
    SetGroupChannel = 0x26,
}

impl MessageType {
    fn is_channel_message(&self) -> bool {
        match IS_CHANNEL_MESSAGE.get(self) {
            Some(&is_ch_msg) => is_ch_msg,
            None => false,
        }
    }
}

lazy_static! {
    static ref IS_CHANNEL_MESSAGE: HashMap<MessageType, bool> = [
        (MessageType::SetupConnection, false),
        (MessageType::SetupConnectionSuccess, false),
        (MessageType::SetupConnectionError, false),
        (MessageType::ChannelEndpointChanged, true),
        (MessageType::OpenStandardMiningChannel, false),
        (MessageType::OpenStandardMiningChannelSuccess, false),
        (MessageType::OpenStandardMiningChannelError, false),
        (MessageType::OpenExtendedMiningChannel, false),
        (MessageType::OpenExtendedMiningChannelSuccess, false),
        (MessageType::OpenExtendedMiningChannelError, false),
        (MessageType::UpdateChannel, true),
        (MessageType::UpdateChannelError, true),
        (MessageType::CloseChannel, true),
        (MessageType::SetExtranoncePrefix, true),
        (MessageType::SubmitSharesStandard, true),
        (MessageType::SubmitSharesExtended, true),
        (MessageType::SubmitSharesSuccess, true),
        (MessageType::SubmitSharesError, true),
        (MessageType::NewMiningJob, true),
        (MessageType::NewExtendedMiningJob, true),
        (MessageType::SetNewPrevHash, true),
        (MessageType::SetTarget, true),
        (MessageType::SetCustomMiningJob, false),
        (MessageType::SetCustomMiningJobSuccess, false),
        (MessageType::SetCustomMiningError, false),
        (MessageType::Reconnect, false),
        (MessageType::SetGroupChannel, false),
    ]
    .iter()
    .cloned()
    .collect();
}

pub const PAYLOAD_CHANNEL_OFFSET: usize = 4;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_header_pack_channel() {
        let expected_bytes = [0x00u8, 0x80, 0x16, 0xcc, 0xbb, 0xaa];
        let header = Header::new(MessageType::UpdateChannel, 0xaabbcc);
        let header_bytes = header.pack_and_swap_endianness();
        assert_eq!(
            expected_bytes, header_bytes,
            "Packing test header failed, message being \
             serialized: {:#08x?}",
            header
        );
    }

    #[test]
    fn test_header_pack_not_channel() {
        let expected_bytes = [0x00u8, 0x00, 0x00, 0xcc, 0xbb, 0xaa];
        let header = Header::new(MessageType::SetupConnection, 0xaabbcc);
        let header_bytes = header.pack_and_swap_endianness();
        assert_eq!(
            expected_bytes, header_bytes,
            "Packing test header failed, message being \
             serialized: {:#08x?}",
            header
        );
    }

    /// This test relies on the fact that there is at least one message type identifier (0xff) is
    /// not used in the protocol.
    #[test]
    fn test_unknown_message_type() {
        let broken_header = [0xffu8, 0xaa, 0xbb, 0xcc];
        let header = Header::unpack_from_slice(&broken_header);
        assert!(
            header.is_err(),
            "Unpacking should have failed to non-existing header type, \
             parsed: {:#04x?}, sliced view {:#04x?}",
            header,
            broken_header
        );
    }
}
