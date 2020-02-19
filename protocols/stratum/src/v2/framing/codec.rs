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

use bytes::{buf::BufMutExt, BytesMut};
use std::convert::TryFrom;
use tokio_util::codec::length_delimited::{self, LengthDelimitedCodec};
use tokio_util::codec::{Decoder, Encoder};

use ii_async_compat::{bytes, tokio_util};

use super::{Frame, Header};
use crate::error::Error;
use crate::v2::noise;

#[derive(Debug)]
pub struct Codec(LengthDelimitedCodec);

impl Decoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let bytes = self.0.decode(src)?;
        let mut bytes = match bytes {
            Some(bytes) => bytes,
            None => return Ok(None),
        };
        Frame::deserialize(&mut bytes).map(Some)
    }
}

impl Encoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        item.serialize(dst)
    }
}

impl Default for Codec {
    fn default() -> Self {
        // TODO: limit frame size with max_frame_length() ?
        Codec(
            length_delimited::Builder::new()
                .little_endian()
                .length_field_offset(Header::LEN_OFFSET)
                .length_field_length(Header::LEN_SIZE)
                .num_skip(0)
                // Actual header length is not counted in the length field
                .length_adjustment(Header::SIZE as isize)
                .new_codec(),
            // Note: LengthDelimitedCodec is a bit tricky to coerce into
            // including the header in the final mesasge.
            // .num_skip(0) tells it to not skip the header,
            // but then .length_adjustment() needs to be set to the header size
            // because normally the 'length' is the size of part after the 'length' field.
        )
    }
}

/// State of the noise codec
#[derive(Debug)]
enum State {
    /// Handshake mode where codec is negotiating keys
    HandShake,
    /// Transport mode where AEAD is fully operational. The `TransportMode` object in this variant
    /// as able to perform encryption and decryption resp.
    Transport(noise::TransportMode),
}

/// Noise codec compound object that stacks length delimited codec and stratum codec
#[derive(Debug)]
pub struct NoiseCodec {
    /// Codec noise messages have simple length delimited framing
    noise_codec: LengthDelimitedCodec,
    stratum_codec: Codec,
    /// Describes mode of operation of the codec
    state: State,
}

impl NoiseCodec {
    const LENGTH_FIELD_OFFSET: usize = 0;
    const LENGTH_FIELD_LENGTH: usize = 2;

    /// Consume the `transport_mode` and set it as the current mode of the codec
    pub fn set_transport_mode(&mut self, transport_mode: noise::TransportMode) {
        if let State::Transport(_) = &self.state {
            panic!("BUG: codec is already in transport mode!");
        }
        self.state = State::Transport(transport_mode);
    }
}

impl Decoder for NoiseCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let noise_msg: Option<BytesMut> = self.noise_codec.decode(src)?;
        let frame = match &mut self.state {
            State::HandShake => {
                let noise_msg = noise_msg.map(|msg| noise::HandShakeMessage::new(msg));
                let noise_frame = noise_msg
                    .map(|msg| Frame::try_from(msg).expect("BUG: cannot create handshake frame"));
                noise_frame
            }
            State::Transport(transport_mode) => match noise_msg {
                Some(msg) => {
                    let mut decrypted_msg = BytesMut::new();
                    transport_mode.read(msg, &mut decrypted_msg)?;
                    self.stratum_codec.decode(&mut decrypted_msg)?
                }
                None => None,
            },
        };
        Ok(frame)
    }
}

impl Encoder for NoiseCodec {
    type Item = Frame;
    type Error = Error;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        let frame_bytes = match &mut self.state {
            State::HandShake => {
                // Intentionally ignore the frame header and serialize just the noise handshake
                // payload
                let mut handshake_frame_writer = BytesMut::new().writer();
                item.payload
                    .serialize_to_writer(&mut handshake_frame_writer)?;
                handshake_frame_writer.into_inner()
            }
            State::Transport(transport_mode) => {
                let mut encoded_frame = BytesMut::new();
                self.stratum_codec.encode(item, &mut encoded_frame)?;
                assert!(
                    encoded_frame.len() <= noise::MAX_MESSAGE_SIZE,
                    "TODO: noise transport doesn't currently support messages bigger than {}",
                    noise::MAX_MESSAGE_SIZE
                );
                let mut encrypted_frame = BytesMut::new();
                transport_mode.write(encoded_frame, &mut encrypted_frame)?;
                encrypted_frame
            }
        };
        self.noise_codec
            .encode(frame_bytes.freeze(), dst)
            .map_err(Into::into)
    }
}

impl Default for NoiseCodec {
    fn default() -> Self {
        // TODO: limit frame size with max_frame_length() ?
        NoiseCodec {
            noise_codec: length_delimited::Builder::new()
                .little_endian()
                .length_field_offset(Self::LENGTH_FIELD_OFFSET)
                .length_field_length(Self::LENGTH_FIELD_LENGTH)
                .new_codec(),
            stratum_codec: Codec::default(),
            state: State::HandShake,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_codec() {
        let mut codec = Codec::default();
        let mut payload = BytesMut::new();
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let expected_frame = Frame::from_serialized_payload(false, 0, 0x16, payload.clone());

        // This is currently due to the fact that Frame doesn't support cloning
        let expected_frame_copy = Frame::from_serialized_payload(false, 0, 0x16, payload);

        let mut buffer = BytesMut::new();
        codec
            .encode(expected_frame, &mut buffer)
            .expect("Codec failed to encode message");

        let decoded_frame = codec
            .decode(&mut buffer)
            .expect("Codec failed to decode message")
            .expect("Incomplete message");

        assert_eq!(
            expected_frame_copy, decoded_frame,
            "Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame_copy, decoded_frame
        );
    }

    #[test]
    fn test_noise_codec_in_handshake_state() {
        let mut codec = NoiseCodec::default();

        let mut payload = BytesMut::new();
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let handshake_msg = noise::HandShakeMessage::new(payload);
        let handshake_frame =
            Frame::try_from(handshake_msg.clone()).expect("BUG: cannot create handshake frame");
        let handshake_frame_copy =
            Frame::try_from(handshake_msg).expect("BUG: cannot create handshake frame");

        let mut buffer = BytesMut::new();
        codec
            .encode(handshake_frame, &mut buffer)
            .expect("BUG: Codec failed to encode message");

        let decoded_frame = codec
            .decode(&mut buffer)
            .expect("BUG: Codec failed to decode message")
            .expect("BUG: Incomplete message");

        assert_eq!(
            handshake_frame_copy, decoded_frame,
            "Expected and decoded frames don't match",
        );
    }

    #[test]
    fn test_noise_codec_in_transport_state() {
        let mut initiator_codec = NoiseCodec::default();
        let mut responder_codec = NoiseCodec::default();

        let (initiator_transport_mode, responder_transport_mode) = noise::test::perform_handshake();

        initiator_codec.set_transport_mode(initiator_transport_mode);
        responder_codec.set_transport_mode(responder_transport_mode);

        let mut payload = BytesMut::new();
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let expected_frame = Frame::from_serialized_payload(false, 0, 0x16, payload.clone());

        // This is currently due to the fact that Frame doesn't support cloning
        let expected_frame_copy = Frame::from_serialized_payload(false, 0, 0x16, payload);

        let mut buffer = BytesMut::new();
        initiator_codec
            .encode(expected_frame, &mut buffer)
            .expect("BUG: Initiator codec failed to encode message");

        let decoded_frame = responder_codec
            .decode(&mut buffer)
            .expect("BUG: Responder codec failed to decode message")
            .expect("BUG: Responder coded provided incomplete message");

        assert_eq!(
            expected_frame_copy, decoded_frame,
            "Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame_copy, decoded_frame
        );
    }
}
