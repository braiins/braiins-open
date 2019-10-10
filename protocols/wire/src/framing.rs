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

use std::fmt::Debug;
use std::ops::Deref;

use ii_async_compat::tokio;
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
    type Tx: Send + Sync;
    /// Receive message type
    type Rx: Send + Sync;
    type Error: From<IOError>;
    type Codec: Encoder<Item = Self::Tx, Error = Self::Error>
        + Decoder<Item = Self::Rx, Error = Self::Error>
        + Default
        + Unpin
        + Send
        + Debug
        + 'static;
}
