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

use ii_async_compat::tokio_util;
use tokio_util::codec::length_delimited::{self, LengthDelimitedCodec};
use tokio_util::codec::{Decoder, Encoder};

use ii_wire::{self, Message, TxFrame};

use super::Header;
use crate::error::Error;
use crate::v2::{deserialize_message, Protocol};

// FIXME: check bytesmut capacity when encoding (use BytesMut::remaining_mut())

#[derive(Debug)]
pub struct Codec(LengthDelimitedCodec);

impl Decoder for Codec {
    type Item = Message<Protocol>;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let bytes = self.0.decode(src)?;
        let bytes = match bytes {
            Some(bytes) => bytes,
            None => return Ok(None),
        };

        deserialize_message(&bytes).map(Some)
    }
}

impl Encoder for Codec {
    type Item = TxFrame;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&item);
        Ok(())
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

#[derive(Debug)]
pub struct Framing;

impl ii_wire::Framing for Framing {
    type Tx = TxFrame;
    type Rx = Message<Protocol>;
    type Error = Error;
    type Codec = Codec;
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use ii_async_compat::tokio;

    use super::*;
    use crate::test_utils::v2::{build_setup_connection, TestIdentityHandler};

    #[tokio::test]
    async fn test_v2codec() {
        let mut codec = Codec::default();

        let input = build_setup_connection();
        let txframe: TxFrame = input
            .clone()
            .try_into()
            .expect("Could not serialize message");

        let mut buffer = BytesMut::new();
        codec
            .encode(txframe, &mut buffer)
            .expect("Codec failed to encode message");

        let msg = codec
            .decode(&mut buffer)
            .expect("Codec failed to decode message")
            .expect("Incomplete message");
        msg.accept(&mut TestIdentityHandler).await;
    }
}
