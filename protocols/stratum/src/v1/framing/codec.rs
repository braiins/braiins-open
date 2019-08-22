use bytes::BytesMut;

use std::str;
use tokio::codec::{Decoder, Encoder, LinesCodec};

use crate::error::Error;
use crate::v1::{deserialize_message, Protocol};
use ii_wire::Message;
use ii_wire::{self, tokio, TxFrame};

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
