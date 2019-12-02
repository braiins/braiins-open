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

use std::convert::TryInto;
use std::io;
use std::marker::PhantomData;
use std::net::TcpListener as StdTcpListener;
use std::net::{Shutdown, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};

use ii_async_compat::prelude::*;
use pin_project::{pin_project, pinned_drop};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::framing::Framing;
use crate::split::{DuplexSplit, TcpDuplexRecv, TcpDuplexSend};

#[pin_project]
#[derive(Debug)]
pub struct ConnectionTx<F: Framing> {
    #[pin]
    inner: FramedWrite<TcpDuplexSend, F::Codec>,
}

impl<F: Framing> ConnectionTx<F> {
    pub async fn send_msg<M, E>(&mut self, message: M) -> Result<(), F::Error>
    where
        F::Error: From<E>,
        M: TryInto<F::Tx, Error = E>,
    {
        let message = message.try_into()?;
        self.send(message).await?;
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.inner.get_ref().local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.inner.get_ref().peer_addr()
    }

    fn do_close(&mut self) {
        let _ = self.inner.get_ref().shutdown(Shutdown::Both);
    }

    pub fn close(mut self) {
        self.do_close();
    }
}

impl<F: Framing> Sink<F::Tx> for ConnectionTx<F> {
    type Error = F::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: F::Tx) -> Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

#[pin_project]
#[derive(Debug)]
pub struct ConnectionRx<F: Framing> {
    #[pin]
    inner: FramedRead<TcpDuplexRecv, F::Codec>,
}

impl<F: Framing> ConnectionRx<F> {
    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.inner.get_ref().local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.inner.get_ref().peer_addr()
    }

    fn do_close(&mut self) {
        let _ = self.inner.get_ref().shutdown(Shutdown::Both);
    }

    pub fn close(mut self) {
        self.do_close();
    }
}

#[pinned_drop]
impl<F: Framing> PinnedDrop for ConnectionRx<F> {
    fn drop(mut self: Pin<&mut Self>) {
        self.do_close();
    }
}

impl<F: Framing> Stream for ConnectionRx<F> {
    type Item = Result<F::Rx, F::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

#[pin_project]
#[derive(Debug)]
pub struct Connection<F: Framing> {
    #[pin]
    pub rx: ConnectionRx<F>,
    #[pin]
    pub tx: ConnectionTx<F>,
}

impl<F: Framing> Connection<F> {
    fn new(stream: TcpStream) -> Self {
        let (stream_rx, stream_tx) = stream.duplex_split();
        let codec_tx = FramedWrite::new(stream_tx, F::Codec::default());
        let codec_rx = FramedRead::new(stream_rx, F::Codec::default());

        let rx = ConnectionRx { inner: codec_rx };
        let tx = ConnectionTx::<F> { inner: codec_tx };

        Self { rx, tx }
    }

    /// Connects to a remote address `addr` and creates two halves
    /// which perfom full message serialization / desrialization
    pub async fn connect(addr: &SocketAddr) -> Result<Self, F::Error> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Connection::new(stream))
    }

    pub async fn send_msg<M, E>(&mut self, message: M) -> Result<(), F::Error>
    where
        F::Error: From<E>,
        M: TryInto<F::Tx, Error = E>,
    {
        let message = message.try_into()?;
        self.tx.send(message).await?;
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.rx.local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.rx.peer_addr()
    }

    pub fn split(self) -> (ConnectionRx<F>, ConnectionTx<F>) {
        (self.rx, self.tx)
    }
}

impl<F: Framing> Stream for Connection<F> {
    type Item = Result<F::Rx, F::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().rx.poll_next(cx)
    }
}

impl<F: Framing> Sink<F::Tx> for Connection<F> {
    type Error = F::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().tx.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: F::Tx) -> Result<(), Self::Error> {
        self.project().tx.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().tx.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().tx.poll_close(cx)
    }
}

#[pin_project]
#[derive(Debug)]
pub struct Server<F: Framing> {
    #[pin]
    tcp: TcpListener,
    _marker: PhantomData<&'static F>,
}

impl<F: Framing> Server<F> {
    pub fn bind(addr: &SocketAddr) -> Result<Server<F>, F::Error> {
        let tcp = StdTcpListener::bind(addr)?;
        let tcp = TcpListener::from_std(tcp)?;

        Ok(Server {
            tcp,
            _marker: PhantomData,
        })
    }
}

impl<F: Framing> Stream for Server<F> {
    type Item = Result<Connection<F>, F::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut tcp = self.project().tcp;

        Pin::new(&mut tcp.incoming())
            .poll_next(cx)
            .map(|opt| opt.map(|res| res.map(Connection::new).map_err(F::Error::from)))
    }
}
