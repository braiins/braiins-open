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

use std::str;

use ii_async_compat::tokio;
use tokio::codec::{Decoder, Encoder, LinesCodec};

use crate::error::Error;
use crate::v1::{deserialize_message, Protocol};
use ii_wire::Message;
use ii_wire::{self, TxFrame};

// FIXME: error handling
// FIXME: check bytesmut capacity when encoding (use BytesMut::remaining_mut())

#[derive(Debug)]
pub struct Codec(LinesCodec);

impl Decoder for Codec {
    type Item = Message<Protocol>;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let line = self.0.decode(src)?;
        match line {
            Some(line) => deserialize_message(&line).map(Some),
            None => Ok(None),
        }
    }
}

impl Encoder for Codec {
    type Item = TxFrame;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let data: &Box<[u8]> = &item;
        self.0
            .encode(str::from_utf8(data)?.to_string(), dst)
            .map_err(Into::into)
    }
}

impl Default for Codec {
    fn default() -> Self {
        // TODO: limit line length with new_with_max_length() ?
        Codec(LinesCodec::new())
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
