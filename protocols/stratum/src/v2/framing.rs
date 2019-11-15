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

use packed_struct::prelude::*;
use packed_struct_codegen::PackedStruct;
use packed_struct_codegen::PrimitiveEnum_u8;

pub mod codec;

/// Header of the protocol message
#[derive(PackedStruct, Debug)]
#[packed_struct(endian = "lsb")]
pub struct Header {
    // WARN: This struct's layout needs to be kept in sync
    // with the consts in the impl block below.
    // This is because the Codec needs to know the offset
    // and size of the msg_length field in the packed byte array
    // (not in the struct in-memory, so we can't auto-deduce this).
    #[packed_field(size_bytes = "1", ty = "enum")]
    pub msg_type: MessageType,
    pub msg_length: Integer<u32, packed_bits::Bits24>,
}

impl Header {
    pub const SIZE: usize = 4;
    pub const LEN_OFFSET: usize = 1;
    pub const LEN_SIZE: usize = 3;

    pub fn new(msg_type: MessageType, msg_length: usize) -> Header {
        assert!(msg_length <= 0xffffff, "Message too large");
        let msg_length = (msg_length as u32).into();

        Header {
            msg_type,
            msg_length,
        }
    }
}

/// All message recognized by the protocol
//#[derive(PrimitiveEnum_u8, Clone, Copy, Debug, PartialEq)]
#[derive(PrimitiveEnum_u8, Clone, Copy, PartialEq, Eq, Debug)]
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

//    // Job Negotiation Protocol
//    AllocateMiningJobToken = 0x50,
//    AllocateMiningJobTokenSuccess = 0x51,
//    AllocateMiningJobTokenError = 0x52,
//    IdentifyTransactions = 0x53,
//    IdentifyTransactionsSuccess = 0x54,
//    ProvideMissingTransaction = 0x55,
//    ProvideMissingTransactionSuccess = 0x56,
//
//    // Template Distribution Protocol
//    CoinbaseOutputDataSize = 0x70,
//    NewTemplate = 0x71,
//    SetNewPrevHash = 0x72,  // Name collision, Template prefix added
//    RequestTransactionData = 0x73,
//    RequestTransactionDataSuccess = 0x74,
//    RequestTransactionDataError = 0x75,
//    SubmitSolution = 0x76,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_header_pack() {
        let expected_bytes = [0x00u8, 0xcc, 0xbb, 0xaa];
        let header = Header::new(MessageType::SetupConnection, 0xaabbcc);
        let header_bytes = header.pack();
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
