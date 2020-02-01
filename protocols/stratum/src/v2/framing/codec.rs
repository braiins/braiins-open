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

#[derive(Debug)]
pub struct Codec(LengthDelimitedCodec);

impl Decoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
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

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
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
}
