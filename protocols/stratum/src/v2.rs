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

use self::messages::MessageType;
use crate::error::Result;

use async_trait::async_trait;
use packed_struct::prelude::*;
use std::convert::TryFrom;

use ii_logging::macros::*;
use ii_wire::{self, Message};

pub use self::framing::codec::{Codec, Framing};
pub use self::framing::Frame;

/// Protocol associates a custom handler with it
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

/// Consumes `frame` and produces a Message object based on the payload type
pub fn build_message_from_frame(frame: framing::Frame) -> Result<Message<Protocol>> {
    trace!("V2: building message from frame {:x?}", frame);

    // Build message based on its type specified in the header
    let (id, payload) = match MessageType::from_primitive(frame.header.msg_type).ok_or(
        error::ErrorKind::UnknownMessage(
            format!("Unexpected payload type, full header: {:x?}", frame.header).into(),
        ),
    )? {
        MessageType::SetupConnection => (
            None,
            Ok(Box::new(messages::SetupConnection::try_from(frame)?)
                as Box<dyn ii_wire::Payload<Protocol>>),
        ),
        MessageType::SetupConnectionSuccess => (
            None,
            Ok(Box::new(messages::SetupConnectionSuccess::try_from(frame)?)
                as Box<dyn ii_wire::Payload<Protocol>>),
        ),
        MessageType::SetupConnectionError => (
            None,
            Ok(Box::new(messages::SetupConnectionError::try_from(frame)?)
                as Box<dyn ii_wire::Payload<Protocol>>),
        ),
        MessageType::OpenStandardMiningChannel => {
            let channel = messages::OpenStandardMiningChannel::try_from(frame)?;
            (
                Some(channel.req_id),
                Ok(Box::new(channel) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::OpenStandardMiningChannelSuccess => {
            let channel_success = messages::OpenStandardMiningChannelSuccess::try_from(frame)?;
            (
                Some(channel_success.req_id),
                Ok(Box::new(channel_success) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::OpenStandardMiningChannelError => {
            let channel_error = messages::OpenStandardMiningChannelError::try_from(frame)?;
            (
                Some(channel_error.req_id),
                Ok(Box::new(channel_error) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::NewMiningJob => {
            let job = messages::NewMiningJob::try_from(frame)?;
            (
                None,
                Ok(Box::new(job) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::SetNewPrevHash => {
            let prev_hash = messages::SetNewPrevHash::try_from(frame)?;
            (
                None,
                Ok(Box::new(prev_hash) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::SetTarget => {
            let target = messages::SetTarget::try_from(frame)?;
            (
                None,
                Ok(Box::new(target) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::SubmitSharesStandard => {
            let submit_shares_standard = messages::SubmitSharesStandard::try_from(frame)?;
            (
                // TODO possibly extract the sequence ID
                None,
                Ok(Box::new(submit_shares_standard) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::SubmitSharesSuccess => {
            let success = messages::SubmitSharesSuccess::try_from(frame)?;
            (
                // TODO what to do about the ID? - use sequence number?
                None,
                Ok(Box::new(success) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        MessageType::SubmitSharesError => {
            let err_msg = messages::SubmitSharesError::try_from(frame)?;
            (
                // TODO what to do about the ID?
                None,
                Ok(Box::new(err_msg) as Box<dyn ii_wire::Payload<Protocol>>),
            )
        }
        _ => (
            None,
            Err(error::ErrorKind::UnknownMessage(
                format!("Unexpected payload type, full header: {:?}", frame.header).into(),
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

    use ii_async_compat::tokio;
    use std::convert::TryInto;

    /// This test demonstrates an actual implementation of protocol handler (aka visitor to a set of
    /// desired messsages
    /// TODO refactor this test once we have a message dispatcher in place
    #[tokio::test]
    async fn test_build_message_from_frame() {
        let message_payload = build_setup_connection();
        let frame = message_payload
            .try_into()
            .expect("Cannot create test frame");

        let message =
            build_message_from_frame(frame).expect("Message payload deserialization failed");
        message.accept(&mut TestIdentityHandler).await;
    }
}
