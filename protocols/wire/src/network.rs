use futures::compat::Future01CompatExt;
use std::convert::TryInto;
use std::io;
use std::marker::PhantomData;
use std::net::{Shutdown, SocketAddr};
use tokio::codec::{FramedRead, FramedWrite};
use tokio::net::{tcp, TcpListener, TcpStream};
use tokio::prelude::*;

use crate::framing::Framing;
use crate::utils::{tcp_split, TcpStreamRecv, TcpStreamSend};

#[derive(Debug)]
pub struct ConnectionTx<F: Framing> {
    inner: FramedWrite<TcpStreamSend, F::Codec>,
}

impl<F: Framing> ConnectionTx<F> {
    pub async fn send<M, E>(&mut self, message: M) -> Result<(), F::Error>
    where
        F::Error: From<E>,
        M: TryInto<F::Tx, Error = E>,
    {
        let message = message.try_into()?;
        await!(self.send_async(message))?;
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

impl<F: Framing> Sink for ConnectionTx<F> {
    type SinkItem = F::Tx;
    type SinkError = F::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> Result<AsyncSink<Self::SinkItem>, F::Error> {
        self.inner.start_send(item)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.inner.poll_complete()
    }
}

#[derive(Debug)]
pub struct ConnectionRx<F: Framing> {
    inner: FramedRead<TcpStreamRecv, F::Codec>,
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

impl<F: Framing> Drop for ConnectionRx<F> {
    fn drop(&mut self) {
        self.do_close();
    }
}

impl<F: Framing> Stream for ConnectionRx<F> {
    type Item = F::Rx;
    type Error = F::Error;

    fn poll(&mut self) -> Result<Async<Option<F::Rx>>, F::Error> {
        self.inner.poll()
    }
}

#[derive(Debug)]
pub struct Connection<F: Framing>(pub ConnectionRx<F>, pub ConnectionTx<F>);

impl<F: Framing> Connection<F> {
    fn new(stream: TcpStream) -> Self {
        let (stream_tx, stream_rx) = tcp_split(stream);
        let codec_tx = FramedWrite::new(stream_tx, F::Codec::default());
        let codec_rx = FramedRead::new(stream_rx, F::Codec::default());

        let conn_rx = ConnectionRx { inner: codec_rx };
        let conn_tx = ConnectionTx::<F> { inner: codec_tx };

        Self(conn_rx, conn_tx)
    }

    /// Connects to a remote address `addr` and creates two halves
    /// which perfom full message serialization / desrialization
    pub async fn connect(addr: &SocketAddr) -> Result<Self, F::Error> {
        let stream = await!(TcpStream::connect(addr).compat())?;
        Ok(Connection::new(stream))
    }

    pub async fn send<M, E>(&mut self, message: M) -> Result<(), F::Error>
    where
        F::Error: From<E>,
        M: TryInto<F::Tx, Error = E>,
    {
        let message = message.try_into()?;
        await!(self.1.send_async(message))?;
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.0.local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.0.peer_addr()
    }

    pub fn split(self) -> (ConnectionRx<F>, ConnectionTx<F>) {
        (self.0, self.1)
    }
}

impl<F: Framing> Stream for Connection<F> {
    type Item = F::Rx;
    type Error = F::Error;

    fn poll(&mut self) -> Result<Async<Option<F::Rx>>, F::Error> {
        self.0.poll()
    }
}

// TODO: not implementing Sink for Connection because it adds
// a conflicting send() method, confusing user code. (But this could be sovled)
// impl<F: Framing> Sink for Connection<F> {
//     type SinkItem = F::Tx;
//     type SinkError = F::Error;

//     fn start_send(&mut self, item: Self::SinkItem) -> Result<AsyncSink<Self::SinkItem>, F::Error> {
//         self.1.start_send(item)
//     }

//     fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
//         self.1.poll_complete()
//     }
// }

#[derive(Debug)]
pub struct Server<F: Framing> {
    tcp: tcp::Incoming,
    _marker: PhantomData<&'static F>,
}

impl<F: Framing> Server<F> {
    pub fn bind(addr: &SocketAddr) -> Result<Server<F>, F::Error> {
        let tcp = TcpListener::bind(addr)?;
        Ok(Server {
            tcp: tcp.incoming(),
            _marker: PhantomData,
        })
    }
}

impl<F: Framing> Stream for Server<F> {
    type Item = Connection<F>;
    type Error = F::Error;

    /// An incoming TCP connection is converted into a new stratum connection with associated receiving codec
    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, F::Error> {
        self.tcp
            .poll()
            .map(|async_res| async_res.map(|stream_opt| stream_opt.map(Connection::new)))
            .map_err(F::Error::from)
    }
}
