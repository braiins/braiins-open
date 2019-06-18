//! Stratum version 2 top level module
pub mod error;
pub mod framing;
pub mod messages;
mod types;

use crate::error::Result;

use crate::v2::framing::MessageType;

use failure::ResultExt;
use packed_struct::PackedStructSlice;
use std::convert::TryFrom;
use wire::{Message, Payload, ProtocolBase};

pub struct V2Protocol;
impl ProtocolBase for V2Protocol {
    type Handler = V2Handler;
}

/// Specifies all messages to be visited
/// TODO document why anything implementing the Handler must be static
pub trait V2Handler: 'static {
    fn visit_setup_mining_connection(
        &mut self,
        _msg: &Message<V2Protocol>,
        _payload: &messages::SetupMiningConnection,
    ) {
    }

    fn visit_setup_mining_connection_success(
        &mut self,
        _msg: &Message<V2Protocol>,
        _payload: &messages::SetupMiningConnectionSuccess,
    ) {
    }
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
    let msg_bytes = &src[framing::Header::SIZE..];

    // Build message based on its type specified in the header
    let payload = match header.msg_type {
        MessageType::SetupMiningConnection => Ok(Box::new(
            messages::SetupMiningConnection::try_from(msg_bytes)?,
        ) as Box<dyn Payload<V2Protocol>>),
        MessageType::SetupMiningConnectionSuccess => Ok(Box::new(
            messages::SetupMiningConnectionSuccess::try_from(msg_bytes)?,
        ) as Box<dyn Payload<V2Protocol>>),
        _ => Err(error::ErrorKind::UnknownMessage(
            format!("Unexpected payload type {:?}", header).into(),
        )
        .into()),
    };
    // TODO: message ID handling is not implemented
    payload.map(|p| Message::new(None, p))
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
        msg.accept(&TestIdentityHandler);
    }
}
