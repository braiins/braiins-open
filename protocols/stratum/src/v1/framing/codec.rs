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

use bytes::{BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder, LinesCodec};

use ii_async_compat::{bytes, tokio_util};

use super::Frame;
use crate::error::Error;

// FIXME: check bytesmut capacity when encoding (use BytesMut::remaining_mut())

/// TODO consider generalizing the codec
#[derive(Debug)]
pub struct Codec(LinesCodec);

impl Decoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let frame_str = self.0.decode(src)?;
        let mut bytes = match frame_str {
            // Note, creating `BytesMut` instance this way creates another copy of the incoming
            // data. We would have to implement a custom decode that would buffer the data
            // directly in 1BytesMut`
            // this copies the frame into the
            Some(frame_str) => BytesMut::from(frame_str.as_bytes()),
            None => return Ok(None),
        };
        Frame::deserialize(&mut bytes).map(Some)
    }
}

impl Encoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.serialize(dst)?;
        dst.put_u8(b'\n');
        Ok(())
    }
}

impl Default for Codec {
    fn default() -> Self {
        // TODO: limit line length with new_with_max_length() ?
        Codec(LinesCodec::new())
    }
}
