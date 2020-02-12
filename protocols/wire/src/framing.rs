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

use ii_async_compat::{tokio, tokio_util};
use tokio::io::Error as IOError;
use tokio_util::codec::{Decoder, Encoder};
pub trait Framing: 'static {
    /// Send message type
    type Tx: Send + Sync;
    /// Receive message type
    type Rx: Send + Sync;
    type Error: From<IOError> + failure::Fail;
    type Codec: Encoder<Item = Self::Tx, Error = Self::Error>
        + Decoder<Item = Self::Rx, Error = Self::Error>
        + Default
        + Unpin
        + Send
        + Debug
        + 'static;
}
