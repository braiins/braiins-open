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

use bytes::BytesMut;
use tokio_util::codec::length_delimited::{self, LengthDelimitedCodec};
use tokio_util::codec::{Decoder, Encoder};

use ii_async_compat::{bytes, tokio_util};

use super::{Frame, Header};
use crate::error::Error;
use crate::v2::noise;

#[derive(Debug)]
pub struct Codec {
    /// Optional noise codec that handles encryption/decryption of messages
    noise_codec: Option<noise::Codec>,
    stratum_codec: LengthDelimitedCodec,
}

impl Codec {
    pub fn new(noise_codec: Option<noise::Codec>) -> Self {
        // TODO: limit frame size with max_frame_length() ?
        // Note: LengthDelimitedCodec is a bit tricky to coerce into
        // including the header in the final mesasge.
        // .num_skip(0) tells it to not skip the header,
        // but then .length_adjustment() needs to be set to the header size
        // because normally the 'length' is the size of part after the 'length' field.
        noise_codec.as_ref().map(|codec| {
            assert!(
                codec.is_in_transport_mode(),
                "BUG: noise codec is not in tranport mode, cannot build V2 Codec!"
            )
        });
        Self {
            noise_codec,
            stratum_codec: length_delimited::Builder::new()
                .little_endian()
                .length_field_offset(Header::LEN_OFFSET)
                .length_field_length(Header::LEN_SIZE)
                .num_skip(0)
                // Actual header length is not counted in the length field
                .length_adjustment(Header::SIZE as isize)
                .new_codec(),
        }
    }
}

impl Default for Codec {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Decoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let stratum_bytes = match self.noise_codec {
            Some(ref mut noise_codec) => noise_codec
                .decode(src)?
                .and_then(|mut noise_bytes| self.stratum_codec.decode(&mut noise_bytes).transpose())
                // If stratum codec has performed decoding we have received Option<Result<>> as
                // an output of the previous transpose (so that it's compatible with and_then).
                // Now perform yet another transpose and bailout upon error
                .transpose()?,
            None => self.stratum_codec.decode(src)?,
        };

        let mut bytes = match stratum_bytes {
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
        let mut encoded_frame = BytesMut::new();
        item.serialize(&mut encoded_frame)?;
        match self.noise_codec {
            Some(ref mut noise_codec) => noise_codec.encode(encoded_frame, dst)?,
            None => dst.unsplit(encoded_frame),
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_codec_no_noise() {
        let mut codec = Codec::default();
        let mut payload = BytesMut::new();
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let expected_frame = Frame::from_serialized_payload(false, 0, 0x16, payload.clone());

        // This is currently due to the fact that Frame doesn't support cloning
        let expected_frame_copy = Frame::from_serialized_payload(false, 0, 0x16, payload);

        let mut buffer = BytesMut::new();
        codec
            .encode(expected_frame, &mut buffer)
            .expect("BUG: Codec failed to encode message");

        let decoded_frame = codec
            .decode(&mut buffer)
            .expect("BUG: Codec failed to decode message")
            .expect("BUG: No frame provided");

        assert_eq!(
            expected_frame_copy, decoded_frame,
            "BUG: Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame_copy, decoded_frame
        );
    }

    /// Attempt to build a V2 codec with noise Codec that is still in handshake mode (=contains
    /// no noise transport) must result in panic
    #[test]
    #[should_panic]
    fn test_bug_on_noise_codec_in_handshake_mode() {
        Codec::new(Some(noise::Codec::default()));
    }

    fn test_codec_with_noise(payload: BytesMut) {
        let mut initiator_noise_codec = noise::Codec::default();
        let mut responder_noise_codec = noise::Codec::default();

        let (initiator_transport_mode, responder_transport_mode) = noise::test::perform_handshake();

        initiator_noise_codec.set_transport_mode(initiator_transport_mode);
        responder_noise_codec.set_transport_mode(responder_transport_mode);

        let mut initiator_codec = Codec::new(Some(initiator_noise_codec));
        let mut responder_codec = Codec::new(Some(responder_noise_codec));

        let expected_frame = Frame::from_serialized_payload(true, 0, 0x16, payload.clone());
        // This is currently due to the fact that Frame doesn't support cloning and it will be
        // consumed by the initiator codec
        let expected_frame_copy = Frame::from_serialized_payload(true, 0, 0x16, payload);
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
            "BUG: Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame_copy, decoded_frame
        );
    }

    #[test]
    fn test_codec_payload_under_noise_max_payload_with_noise() {
        let mut payload = BytesMut::new();
        payload.extend_from_slice(&[1, 2, 3, 4]);

        assert!(
            payload.len() <= noise::MAX_PAYLOAD_SIZE,
            "BUG: cannot test, provided frame must be <= noise::MAX_PAYLOAD"
        );
        test_codec_with_noise(payload);
    }

    /// TODO: remove the expected panic once we have support for bigger payload
    #[test]
    #[should_panic]
    fn test_codec_payload_over_noise_max_payload_with_noise_() {
        let payload = super::super::test::build_large_payload(Header::MAX_LEN as usize);
        assert!(
            payload.len() > noise::MAX_PAYLOAD_SIZE,
            "BUG: cannot test, provided frame must exceed noise::MAX_PAYLOAD"
        );
        test_codec_with_noise(payload);
    }
}
