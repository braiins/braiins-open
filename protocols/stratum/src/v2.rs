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

use self::framing::MessageType;
use crate::error::{Result, ResultExt};

use async_trait::async_trait;
use packed_struct::prelude::*;
use std::convert::TryFrom;

use ii_logging::macros::*;
use ii_wire::{self, Message, Payload};

pub use self::framing::codec::{Codec, Framing};
pub use self::framing::TxFrame;

pub struct Protocol;
impl ii_wire::Protocol for Protocol {
    type Handler = dyn Handler;
}

/// Specifies all messages to be visited
/// TODO document why anything implementing the Handler must be static
#[async_trait]
pub trait Handler: 'static + Send {
    async fn visit_setup_connection(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SetupConnection,
    ) {
    }

    async fn visit_setup_connection_success(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SetupConnectionSuccess,
    ) {
    }

    async fn visit_setup_connection_error(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SetupConnectionError,
    ) {
    }

    async fn visit_open_standard_mining_channel(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::OpenStandardMiningChannel,
    ) {
    }

    async fn visit_open_standard_mining_channel_success(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::OpenStandardMiningChannelSuccess,
    ) {
    }

    async fn visit_open_standard_mining_channel_error(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::OpenStandardMiningChannelError,
    ) {
    }

    async fn visit_update_channel(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::UpdateChannel,
    ) {
    }

    async fn visit_update_channel_error(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::UpdateChannelError,
    ) {
    }

    async fn visit_submit_shares_standard(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SubmitSharesStandard,
    ) {
    }

    async fn visit_submit_shares_success(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SubmitSharesSuccess,
    ) {
    }

    async fn visit_submit_shares_error(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SubmitSharesError,
    ) {
    }

    async fn visit_new_mining_job(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::NewMiningJob,
    ) {
    }

    async fn visit_set_new_prev_hash(
        &mut self,
        _msg: &Message<Protocol>,
        _payload: &messages::SetNewPrevHash,
    ) {
    }

    async fn visit_set_target(&mut self, _msg: &Message<Protocol>, _payload: &messages::SetTarget) {
    }
}

/// TODO should/could this be part of the framing trait or protocol trait or none of these
/// (implement From trait...)
pub fn deserialize_message(src: &[u8]) -> Result<Message<Protocol>> {
    let header = framing::Header::unpack_and_swap_endianness(&src[0..framing::Header::SIZE])
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
    let (id, payload) = match MessageType::from_primitive(header.msg_type).ok_or(
        error::ErrorKind::UnknownMessage(
            format!("Unexpected payload type, full header: {:?}", header).into(),
        ),
    )? {
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
            let channel_error = messages::OpenStandardMiningChannelError::try_from(msg_bytes)?;
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
            let target = messages::SetTarget::try_from(msg_bytes)?;
            (None, Ok(Box::new(target) as Box<dyn Payload<Protocol>>))
        }
        MessageType::SubmitSharesStandard => {
            let submit_shares_standard = messages::SubmitSharesStandard::try_from(msg_bytes)?;
            (
                // TODO possibly extract the sequence ID
                None,
                Ok(Box::new(submit_shares_standard) as Box<dyn Payload<Protocol>>),
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
    use crate::test_utils::v2::*;

    use bytes::BytesMut;
    use ii_async_compat::{bytes, tokio};

    /// This test demonstrates an actual implementation of protocol handler (aka visitor to a set of
    /// desired messsages
    #[tokio::test]
    async fn test_deserialize_message() {
        // build serialized message
        let header = framing::Header::new(
            framing::MessageType::SetupConnection,
            SETUP_CONNECTION_SERIALIZED.len(),
        );
        let mut serialized_msg = BytesMut::with_capacity(64);
        serialized_msg.extend_from_slice(&header.pack_and_swap_endianness());
        serialized_msg.extend_from_slice(SETUP_CONNECTION_SERIALIZED);

        let msg = deserialize_message(&serialized_msg).expect("Deserialization failed");
        msg.accept(&mut TestIdentityHandler).await;
    }
}
