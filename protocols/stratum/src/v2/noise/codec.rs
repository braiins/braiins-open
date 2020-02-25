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

//! Noise protocol codec implementation that takes care of framing

use bytes::BytesMut;
use tokio_util::codec::length_delimited::{self, LengthDelimitedCodec};
use tokio_util::codec::{Decoder, Encoder};

use ii_async_compat::{bytes, tokio_util};

use crate::error::Error;

use super::TransportMode;

/// State of the noise codec
#[derive(Debug)]
enum State {
    /// Handshake mode where codec is negotiating keys
    HandShake,
    /// Transport mode where AEAD is fully operational. The `TransportMode` object in this variant
    /// as able to perform encryption and decryption resp.
    Transport(TransportMode),
}

/// Noise codec compound object that stacks length delimited codec and stratum codec
#[derive(Debug)]
pub struct Codec {
    /// Codec noise messages have simple length delimited framing
    codec: LengthDelimitedCodec,
    /// Describes mode of operation of the codec
    state: State,
}

impl Codec {
    const LENGTH_FIELD_OFFSET: usize = 0;
    const LENGTH_FIELD_LENGTH: usize = 2;

    /// Consume the `transport_mode` and set it as the current mode of the codec
    pub fn set_transport_mode(&mut self, transport_mode: TransportMode) {
        if let State::Transport(_) = &self.state {
            panic!("BUG: codec is already in transport mode!");
        }
        self.state = State::Transport(transport_mode);
    }

    pub fn is_in_transport_mode(&self) -> bool {
        match self.state {
            State::Transport(_) => true,
            _ => false,
        }
    }
}

impl Decoder for Codec {
    type Item = BytesMut;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let noise_msg: Option<BytesMut> = self.codec.decode(src)?;
        let payload = match &mut self.state {
            State::HandShake => noise_msg,
            State::Transport(transport_mode) => match noise_msg {
                Some(msg) => {
                    let mut decrypted_msg = BytesMut::new();
                    transport_mode.read(msg, &mut decrypted_msg)?;
                    Some(decrypted_msg)
                }
                None => None,
            },
        };
        Ok(payload)
    }
}

impl Encoder for Codec {
    type Item = BytesMut;
    type Error = Error;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        let payload = match &mut self.state {
            State::HandShake => item,
            State::Transport(transport_mode) => {
                assert!(
                    item.len() <= super::MAX_PAYLOAD_SIZE,
                    "TODO: noise transport doesn't currently support messages bigger than {} \
                     (requested: {})",
                    super::MAX_PAYLOAD_SIZE,
                    item.len()
                );
                let mut encrypted_payload = BytesMut::new();
                transport_mode.write(item, &mut encrypted_payload)?;
                encrypted_payload
            }
        };
        self.codec.encode(payload.freeze(), dst).map_err(Into::into)
    }
}

impl Default for Codec {
    fn default() -> Self {
        // TODO: limit frame size with max_frame_length() ?
        Codec {
            codec: length_delimited::Builder::new()
                .little_endian()
                .length_field_offset(Self::LENGTH_FIELD_OFFSET)
                .length_field_length(Self::LENGTH_FIELD_LENGTH)
                .new_codec(),
            state: State::HandShake,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_noise_codec_in_handshake_state() {
        let mut codec = Codec::default();
        let mut expected_frame = BytesMut::new();
        expected_frame.extend_from_slice(&[1, 2, 3, 4]);

        let mut buffer = BytesMut::new();
        codec
            .encode(expected_frame.clone(), &mut buffer)
            .expect("Codec failed to encode message");

        let decoded_frame = codec
            .decode(&mut buffer)
            .expect("Codec failed to decode message")
            .expect("Incomplete message");

        assert_eq!(
            expected_frame, decoded_frame,
            "Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame, decoded_frame
        );
    }

    #[test]
    fn test_noise_codec_in_transport_state() {
        let mut initiator_codec = Codec::default();
        let mut responder_codec = Codec::default();

        let (initiator_transport_mode, responder_transport_mode) =
            super::super::test::perform_handshake();

        initiator_codec.set_transport_mode(initiator_transport_mode);
        responder_codec.set_transport_mode(responder_transport_mode);

        let mut expected_frame = BytesMut::new();
        expected_frame.extend_from_slice(&[1, 2, 3, 4]);

        let mut buffer = BytesMut::new();
        initiator_codec
            .encode(expected_frame.clone(), &mut buffer)
            .expect("BUG: Initiator codec failed to encode message");

        let decoded_frame = responder_codec
            .decode(&mut buffer)
            .expect("BUG: Responder codec failed to decode message")
            .expect("BUG: Responder coded provided incomplete message");

        assert_eq!(
            expected_frame, decoded_frame,
            "Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame, decoded_frame
        );
    }
}
