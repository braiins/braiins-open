//! Stratum version 2 top level module
pub mod error;
pub mod framing;
pub mod messages;
pub mod types;

use crate::error::Result;

use crate::v2::framing::MessageType;
use crate::LOGGER;

use failure::ResultExt;
use packed_struct::PackedStructSlice;
use slog::trace;
use std::convert::TryFrom;
use wire::{Message, Payload, ProtocolBase};

pub struct V2Protocol;
impl ProtocolBase for V2Protocol {
    type Handler = V2Handler;
}

macro_rules! handler_method {
    ($ty:ident, $name:ident) => (
        fn $name(
            &mut self,
            _msg: &Message<V2Protocol>,
            _payload: &messages::$ty,
        ) {}
    )
}

/// Specifies all messages to be visited
/// TODO document why anything implementing the Handler must be static
pub trait V2Handler: 'static {
    handler_method!(SetupMiningConnection, visit_setup_mining_connection);
    handler_method!(
        SetupMiningConnectionSuccess,
        visit_setup_mining_connection_success
    );
    handler_method!(
        SetupMiningConnectionError,
        visit_setup_mining_connection_error
    );
    handler_method!(OpenChannel, visit_open_channel);
    handler_method!(OpenChannelSuccess, visit_open_channel_success);
    handler_method!(OpenChannelError, visit_open_channel_error);
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
pub fn deserialize_message(src: &[u8]) -> Result<Message<V2Protocol>> {
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
    trace!(LOGGER, "V2: deserialized header: {:?}", header);
    let msg_bytes = &src[framing::Header::SIZE..];

    // Build message based on its type specified in the header
    let (id, payload) = match header.msg_type {
        MessageType::SetupMiningConnection => (
            None,
            Ok(
                Box::new(messages::SetupMiningConnection::try_from(msg_bytes)?)
                    as Box<dyn Payload<V2Protocol>>,
            ),
        ),
        MessageType::SetupMiningConnectionSuccess => (
            None,
            Ok(
                Box::new(messages::SetupMiningConnectionSuccess::try_from(msg_bytes)?)
                    as Box<dyn Payload<V2Protocol>>,
            ),
        ),
        MessageType::SetupMiningConnectionError => (
            None,
            Ok(
                Box::new(messages::SetupMiningConnectionError::try_from(msg_bytes)?)
                    as Box<dyn Payload<V2Protocol>>,
            ),
        ),
        MessageType::OpenChannel => {
            let channel = messages::OpenChannel::try_from(msg_bytes)?;
            (
                Some(channel.req_id),
                Ok(Box::new(channel) as Box<dyn Payload<V2Protocol>>),
            )
        }
        MessageType::OpenChannelSuccess => {
            let channel_success = messages::OpenChannelSuccess::try_from(msg_bytes)?;
            (
                Some(channel_success.req_id),
                Ok(Box::new(channel_success) as Box<dyn Payload<V2Protocol>>),
            )
        }
        MessageType::OpenChannelError => {
            let channel_error = messages::OpenChannelSuccess::try_from(msg_bytes)?;
            (
                Some(channel_error.req_id),
                Ok(Box::new(channel_error) as Box<dyn Payload<V2Protocol>>),
            )
        }
        MessageType::NewMiningJob => {
            let job = messages::NewMiningJob::try_from(msg_bytes)?;
            (None, Ok(Box::new(job) as Box<dyn Payload<V2Protocol>>))
        }
        MessageType::SetNewPrevHash => {
            let prev_hash = messages::SetNewPrevHash::try_from(msg_bytes)?;
            (
                None,
                Ok(Box::new(prev_hash) as Box<dyn Payload<V2Protocol>>),
            )
        }
        MessageType::SubmitShares => {
            let submit_shares = messages::SubmitShares::try_from(msg_bytes)?;
            (
                // TODO possibly extract the sequence ID
                None,
                Ok(Box::new(submit_shares) as Box<dyn Payload<V2Protocol>>),
            )
        }
        MessageType::SubmitSharesSuccess => {
            let success = messages::SubmitSharesSuccess::try_from(msg_bytes)?;
            (
                // TODO what to do about the ID? - use sequence number?
                None,
                Ok(Box::new(success) as Box<dyn Payload<V2Protocol>>),
            )
        }
        MessageType::SubmitSharesError => {
            let err_msg = messages::SubmitSharesError::try_from(msg_bytes)?;
            (
                // TODO what to do about the ID?
                None,
                Ok(Box::new(err_msg) as Box<dyn Payload<V2Protocol>>),
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
    trace!(LOGGER, "V2: message ID: {:?}", id);
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
            framing::MessageType::SetupMiningConnection,
            SETUP_MINING_CONNECTION_SERIALIZED.len(),
        );
        let mut serialized_msg = BytesMut::with_capacity(64);
        serialized_msg.extend_from_slice(&header.pack());
        serialized_msg.extend_from_slice(SETUP_MINING_CONNECTION_SERIALIZED.as_bytes());

        let msg = deserialize_message(&serialized_msg).expect("Deserialization failed");
        msg.accept(&mut TestIdentityHandler);
    }
}
