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

//! Stratum version 2 top level module
pub mod error;
pub mod framing;
pub mod messages;
pub mod serialization;
pub mod types;

use crate::error::{Result, ResultExt};

use crate::v2::framing::MessageType;

use packed_struct::PackedStructSlice;
use std::convert::TryFrom;

use ii_logging::macros::*;
use ii_wire::{self, Message, Payload};

pub use self::framing::codec::{Codec, Framing};

pub struct Protocol;
impl ii_wire::Protocol for Protocol {
    type Handler = Handler;
}

macro_rules! handler_method {
    ($ty:ident, $name:ident) => (
        fn $name(
            &mut self,
            _msg: &Message<Protocol>,
            _payload: &messages::$ty,
        ) {}
    )
}

/// Specifies all messages to be visited
/// TODO document why anything implementing the Handler must be static
pub trait Handler: 'static {
    handler_method!(SetupConnection, visit_setup_connection);
    handler_method!(SetupConnectionSuccess, visit_setup_connection_success);
    handler_method!(SetupConnectionError, visit_setup_connection_error);
    handler_method!(
        OpenStandardMiningChannel,
        visit_open_standard_mining_channel
    );
    handler_method!(
        OpenStandardMiningChannelSuccess,
        visit_open_standard_mining_channel_success
    );
    handler_method!(
        OpenStandardMiningChannelError,
        visit_open_standard_mining_channel_error
    );
    handler_method!(UpdateChannel, visit_update_channel);
    handler_method!(UpdateChannelError, visit_update_channel_error);
    handler_method!(SubmitShares, visit_submit_shares);
    handler_method!(SubmitSharesSuccess, visit_submit_shares_success);
    handler_method!(SubmitSharesError, visit_submit_shares_error);
    handler_method!(NewMiningJob, visit_new_mining_job);
    handler_method!(SetNewPrevHash, visit_set_new_prev_hash);
    handler_method!(SetTarget, visit_set_target);
}

/// TODO should/could this be part of the framing trait or protocol trait or none of these
/// (implement From trait...)
pub fn deserialize_message(src: &[u8]) -> Result<Message<Protocol>> {
    let header = framing::Header::unpack_from_slice(&src[0..framing::Header::SIZE])
        .context("Cannot decode V2 header")?;
    // Decoder should have ensured correct framing. This is only sanity check, therefore we don't
    // convert it into an error as it is effectively a bug!
    let msg_len: u32 = header.msg_length.into();
    assert_eq!(
        framing::Header::SIZE + msg_len as usize,
        src.len(),
        "Malformed message"
    );
    trace!("V2: deserialized header: {:?}", header);
    let msg_bytes = &src[framing::Header::SIZE..];

    // Build message based on its type specified in the header
    let (id, payload) = match header.msg_type {
        MessageType::SetupConnection => (
            None,
            Ok(Box::new(messages::SetupConnection::try_from(msg_bytes)?)
                as Box<dyn Payload<Protocol>>),
        ),
        MessageType::SetupConnectionSuccess => (
            None,
            Ok(
                Box::new(messages::SetupConnectionSuccess::try_from(msg_bytes)?)
                    as Box<dyn Payload<Protocol>>,
            ),
        ),
        MessageType::SetupConnectionError => (
            None,
            Ok(
                Box::new(messages::SetupConnectionError::try_from(msg_bytes)?)
                    as Box<dyn Payload<Protocol>>,
            ),
        ),
        MessageType::OpenStandardMiningChannel => {
            let channel = messages::OpenStandardMiningChannel::try_from(msg_bytes)?;
            (
                Some(channel.req_id),
                Ok(Box::new(channel) as Box<dyn Payload<Protocol>>),
            )
        }
        MessageType::OpenStandardMiningChannelSuccess => {
            let channel_success = messages::OpenStandardMiningChannelSuccess::try_from(msg_bytes)?;
            (
                Some(channel_success.req_id),
                Ok(Box::new(channel_success) as Box<dyn Payload<Protocol>>),
            )
        }
        MessageType::OpenStandardMiningChannelError => {
            let channel_error = messages::OpenStandardMiningChannelSuccess::try_from(msg_bytes)?;
            (
                Some(channel_error.req_id),
                Ok(Box::new(channel_error) as Box<dyn Payload<Protocol>>),
            )
        }
        MessageType::NewMiningJob => {
            let job = messages::NewMiningJob::try_from(msg_bytes)?;
            (None, Ok(Box::new(job) as Box<dyn Payload<Protocol>>))
        }
        MessageType::SetNewPrevHash => {
            let prev_hash = messages::SetNewPrevHash::try_from(msg_bytes)?;
            (None, Ok(Box::new(prev_hash) as Box<dyn Payload<Protocol>>))
        }
        MessageType::SetTarget => {
            let prev_hash = messages::SetTarget::try_from(msg_bytes)?;
            (None, Ok(Box::new(prev_hash) as Box<dyn Payload<Protocol>>))
        }
        MessageType::SubmitShares => {
            let submit_shares = messages::SubmitShares::try_from(msg_bytes)?;
            (
                // TODO possibly extract the sequence ID
                None,
                Ok(Box::new(submit_shares) as Box<dyn Payload<Protocol>>),
            )
        }
        MessageType::SubmitSharesSuccess => {
            let success = messages::SubmitSharesSuccess::try_from(msg_bytes)?;
            (
                // TODO what to do about the ID? - use sequence number?
                None,
                Ok(Box::new(success) as Box<dyn Payload<Protocol>>),
            )
        }
        MessageType::SubmitSharesError => {
            let err_msg = messages::SubmitSharesError::try_from(msg_bytes)?;
            (
                // TODO what to do about the ID?
                None,
                Ok(Box::new(err_msg) as Box<dyn Payload<Protocol>>),
            )
        }
        _ => (
            None,
            Err(error::ErrorKind::UnknownMessage(
                format!("Unexpected payload type, full header: {:?}", header).into(),
            )
            .into()),
        ),
    };
    trace!("V2: message ID: {:?}", id);
    // TODO: message ID handling is not implemented
    payload.map(|p| Message::new(id, p))
}

#[cfg(test)]
pub mod test {
    use super::*;
    use bytes::BytesMut;
    use packed_struct::PackedStruct;

    use crate::test_utils::v2::*;

    /// This test demonstrates an actual implementation of protocol handler (aka visitor to a set of
    /// desired messsages
    #[test]
    fn test_deserialize_message() {
        // build serialized message
        let header = framing::Header::new(
            framing::MessageType::SetupConnection,
            SETUP_CONNECTION_SERIALIZED.len(),
        );
        let mut serialized_msg = BytesMut::with_capacity(64);
        serialized_msg.extend_from_slice(&header.pack());
        serialized_msg.extend_from_slice(SETUP_CONNECTION_SERIALIZED);

        let msg = deserialize_message(&serialized_msg).expect("Deserialization failed");
        msg.accept(&mut TestIdentityHandler);
    }
}
