use std::ops::Deref;
use tokio::codec::{Decoder, Encoder};
use tokio::io::Error as IOError;

/// Represents a generic frame being sent/received.
#[derive(PartialEq, Debug)]
pub struct Frame<T>(T);

impl<T> Frame<T> {
    pub fn new(data: T) -> Self {
        Self(data)
    }
}

// TODO: to be reviewed/removed. The idea was to have a generic representation of Rx and Tx frame
pub type TxFrame = Frame<Box<[u8]>>;
//pub type RxFrame<'a> = Frame<'a, &'a [u8]>;

/// Add dereferencing
impl<T> Deref for Frame<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

//impl AsRef<[u8]> for TxFrame {
//    fn as_ref(&self) -> &[u8] {
//        &self.0
//    }
//}
//
///// Sliced frame is used when receiving
//impl AsRef<&[u8]> for RxFrame {
//    fn as_ref(&self) -> &[u8] {
//        self.0
//    }
//}

/// TODO: review the Send/Receive associated types as for reception we pretty much have
/// Message<Protocol> and for sending we have TxFrame. We should make this a bit more uniform
pub trait Framing: 'static {
    /// Send message type
    type Send: Send + Sync;
    /// Receive message type
    type Receive: Send + Sync;
    type Error: From<IOError>;
    type Codec: Encoder<Item = Self::Send, Error = Self::Error>
        + Decoder<Item = Self::Receive, Error = Self::Error>
        + Default
        + Unpin
        + Send
        + 'static;
}
