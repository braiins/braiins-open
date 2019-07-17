use bytes::BytesMut;

use tokio::codec::length_delimited::{self, LengthDelimitedCodec};
use tokio::codec::{Decoder, Encoder};

use wire::tokio;
use wire::{self, Message, TxFrame};

use super::Header;
use crate::error::Error;
use crate::v2::{deserialize_message, Protocol};

// FIXME: error handling
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
            // TODO: numbers vs constants
            length_delimited::Builder::new()
                .little_endian()
                .length_field_offset(1)
                .length_field_length(3)
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

impl wire::Framing for Framing {
    type Tx = TxFrame;
    type Rx = Message<Protocol>;
    type Error = Error;
    type Codec = Codec;
}
