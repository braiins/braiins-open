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

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use ii_async_compat::prelude::*;
use pin_project::pin_project;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_util::codec::Framed;

use crate::framing::Framing;

#[pin_project]
#[derive(Debug)]
pub struct Connection<F: Framing> {
    #[pin]
    pub framed_stream: Framed<TcpStream, F::Codec>,
}

impl<F: Framing> Connection<F> {
    /// Create a new `Connection` from an existing TCP stream
    pub fn new(stream: TcpStream) -> Self {
        let framed_stream = Framed::new(stream, F::Codec::default());

        Self { framed_stream }
    }

    pub fn codec_mut(&mut self) -> &mut F::Codec {
        self.framed_stream.codec_mut()
    }

    /// Connects to a remote address `addr` and creates two halves
    /// which perfom full message serialization / desrialization
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self, F::Error> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Connection::new(stream))
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.framed_stream.get_ref().local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.framed_stream.get_ref().peer_addr()
    }

    pub fn into_inner(self) -> Framed<TcpStream, F::Codec> {
        self.framed_stream
    }
}

impl<F: Framing> From<TcpStream> for Connection<F> {
    fn from(stream: TcpStream) -> Self {
        Self::new(stream)
    }
}

impl<F: Framing> Stream for Connection<F> {
    type Item = Result<F::Rx, F::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().framed_stream.poll_next(cx)
    }
}

impl<F: Framing> Sink<F::Tx> for Connection<F> {
    type Error = F::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().framed_stream.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: F::Tx) -> Result<(), Self::Error> {
        self.project().framed_stream.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().framed_stream.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().framed_stream.poll_close(cx)
    }
}
