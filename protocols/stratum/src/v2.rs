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

//! Stratum version 2 top level module
pub mod error;
pub mod framing;
#[macro_use]
pub mod macros;
pub mod extensions;
pub mod messages;
pub mod noise;
pub mod serialization;
pub mod telemetry;
pub mod types;

use tokio::net::TcpStream;

use futures::prelude::*;
use ii_wire;

pub use self::framing::codec::Codec;
pub use self::framing::{Frame, Framing};

/// Tcp stream that produces/consumes V2 frames
pub type Framed = tokio_util::codec::Framed<TcpStream, self::noise::CompoundCodec<Codec>>;

pub trait FramedSink:
    Sink<<Framing as ii_wire::Framing>::Tx, Error = <Framing as ii_wire::Framing>::Error>
    + std::marker::Unpin
    + std::fmt::Debug
    + 'static
{
}

impl<T> FramedSink for T where
    T: Sink<<Framing as ii_wire::Framing>::Tx, Error = <Framing as ii_wire::Framing>::Error>
        + std::marker::Unpin
        + std::fmt::Debug
        + 'static
{
}

/// Helper type for outgoing V2 frames when run time support for multiple sink types (e.g.
/// TcpStream, mpsc::Sender etc.) is needed
pub type DynFramedSink = std::pin::Pin<
    Box<
        dyn Sink<<Framing as ii_wire::Framing>::Tx, Error = <Framing as ii_wire::Framing>::Error>
            + Send
            + std::marker::Unpin,
    >,
>;

pub trait FramedStream:
    Stream<
        Item = std::result::Result<
            <Framing as ii_wire::Framing>::Tx,
            <Framing as ii_wire::Framing>::Error,
        >,
    > + std::marker::Unpin
    + 'static
{
}

impl<T> FramedStream for T where
    T: Stream<
            Item = std::result::Result<
                <Framing as ii_wire::Framing>::Tx,
                <Framing as ii_wire::Framing>::Error,
            >,
        > + std::marker::Unpin
        + 'static
{
}

/// Helper type for incoming V2 frames when run time support for multiple sources (e.g.
/// TcpStream, mpsc::Receiver etc.) is needed
pub type DynFramedStream = std::pin::Pin<
    Box<
        dyn Stream<
                Item = std::result::Result<
                    <Framing as ii_wire::Framing>::Rx,
                    <Framing as ii_wire::Framing>::Error,
                >,
            > + Send,
    >,
>;

/// Protocol associates a custom handler with it
pub struct Protocol;
impl crate::Protocol for Protocol {
    type Header = framing::Header;
}

#[cfg(test)]
mod test;
