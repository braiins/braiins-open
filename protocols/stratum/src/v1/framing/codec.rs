use bytes::BytesMut;

use std::str;
use tokio::codec::{Decoder, Encoder, LinesCodec};

use crate::error::Error;
use crate::v1::{deserialize_message, V1Protocol};
use wire::Message;
use wire::{self, tokio, TxFrame};

// FIXME: error handling
// FIXME: check bytesmut capacity when encoding (use BytesMut::remaining_mut())

#[derive(Debug)]
pub struct V1Codec(LinesCodec);

impl Decoder for V1Codec {
    type Item = Message<V1Protocol>;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let line = self.0.decode(src)?;
        match line {
            Some(line) => deserialize_message(&line).map(Some),
            None => Ok(None),
        }
    }
}

impl Encoder for V1Codec {
    type Item = TxFrame;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let data: &Box<[u8]> = &item;
        self.0
            .encode(str::from_utf8(data)?.to_string(), dst)
            .map_err(Into::into)
    }
}

impl Default for V1Codec {
    fn default() -> Self {
        V1Codec(LinesCodec::new())
    }
}

#[derive(Debug)]
pub struct V1Framing;

impl wire::Framing for V1Framing {
    type Send = TxFrame;
    type Receive = Message<V1Protocol>;
    type Error = Error;
    type Codec = V1Codec;
}
