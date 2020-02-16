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

//! All shared public macros for the V2 part of the stack

/// Generates conversion for a specified protocol message
/// `extension_id` - Identifies the protocol extension
/// `message` - Message identifier (token)
/// `is_channel_msg` - expected boolean value whether the message is a channel message
/// `handler_fn` - handler method that is to be called after accept
#[macro_export]
macro_rules! impl_message_conversion {
    ($extension_id:tt, $message:tt, $is_channel_msg:expr, $handler_fn:tt) => {
        // NOTE: $message and $handler_fn need to be tt because of https://github.com/dtolnay/async-trait/issues/46

        impl TryFrom<$message> for framing::Frame {
            type Error = Error;

            /// Prepares a frame for serializing the specified message just in time (the message
            /// is treated as a `SerializablePayload`)
            fn try_from(m: $message) -> Result<Self> {
                Ok(framing::Frame::from_serializable_payload(
                    $is_channel_msg,
                    $extension_id,
                    MessageType::$message as framing::MsgType,
                    m,
                ))
            }
        }

        impl TryFrom<&[u8]> for $message {
            type Error = Error;

            fn try_from(msg: &[u8]) -> Result<Self> {
                serialization::from_slice(msg).map_err(Into::into)
            }
        }

        impl TryFrom<framing::Frame> for $message {
            type Error = Error;

            fn try_from(frame: framing::Frame) -> Result<Self> {
                let (_header, payload) = frame.split();
                let payload = payload.into_bytes_mut()?;
                Self::try_from(&payload[..])
            }
        }

        /// Each message is a `AnyPayload/SerializablePayload` object that can be serialized into
        /// `writer`
        #[async_trait]
        impl AnyPayload<Protocol> for $message {
            async fn accept(
                &self,
                header: &<Protocol as crate::Protocol>::Header,
                handler: &mut <Protocol as crate::Protocol>::Handler,
            ) {
                handler.$handler_fn(header, self).await;
            }

            fn serialize_to_writer(&self, writer: &mut dyn std::io::Write) -> Result<()> {
                serialization::to_writer(writer, self).map_err(Into::into)
            }
        }
    };
}
