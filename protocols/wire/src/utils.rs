use std::io;
use std::net::{Shutdown, SocketAddr};

use futures_01::sync::BiLock;
use futures_01::{try_ready, Async};

use tokio::net::TcpStream;
use tokio::prelude::{AsyncRead, AsyncWrite, Poll};

/// This is a newtype uniting unix `RawFd` and windows `RawSocket`,
/// implementing local & peer addr getters for use in `TcpStreamRecv` and `TcpStreamSend`.
#[cfg(target_family = "unix")]
mod raw_fd {
    use std::io;
    use std::net::TcpStream as StdStream;
    use std::net::{Shutdown, SocketAddr};
    use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
    use tokio::net::TcpStream as TokioStream;

    #[derive(Clone, Copy, Debug)]
    pub struct Fd(RawFd);

    impl<'a> From<&'a TokioStream> for Fd {
        fn from(stream: &'a TokioStream) -> Fd {
            Fd(stream.as_raw_fd())
        }
    }

    impl Fd {
        // WARN: It's imperative to convert the stream back into raw fd
        // after using it, this prevents its drop() from closing the socket.

        pub fn shutdown(&self, how: Shutdown) -> Result<(), io::Error> {
            let stream = unsafe { StdStream::from_raw_fd(self.0) };
            let res = stream.shutdown(how);
            let _ = stream.into_raw_fd();
            res
        }

        pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
            let stream = unsafe { StdStream::from_raw_fd(self.0) };
            let res = stream.local_addr();
            let _ = stream.into_raw_fd();
            res
        }

        pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
            let stream = unsafe { StdStream::from_raw_fd(self.0) };
            let res = stream.peer_addr();
            // Converting the stream back into raw fd prevents its drop()
            // from closing the socket:
            let _ = stream.into_raw_fd();
            res
        }
    }
}

#[cfg(target_family = "windows")]
mod raw_fd {
    use std::io;
    use std::net::TcpStream as StdStream;
    use std::net::{Shutdown, SocketAddr};
    use std::os::windows::io::{AsRawSocket, IntoRawSocket, RawSocket};
    use tokio::net::TcpStream as TokioStream;

    #[derive(Clone, Copy, Debug)]
    pub struct Fd(RawSocket);

    impl<'a> From<&'a TcpStream> for Fd {
        fn from(stream: &'a TcpStream) -> Fd {
            Fd(stream.as_raw_socket())
        }
    }

    impl Fd {
        // WARN: It's imperative to convert the stream back into raw fd
        // after using it, this prevents its drop() from closing the socket.

        pub fn shutdown(&self, how: Shutdown) -> Result<(), io::Error> {
            let stream = unsafe { StdStream::from_raw_socket(self.0) };
            let res = stream.shutdown(how);
            let _ = stream.into_raw_socket();
            res
        }

        pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
            let stream = unsafe { StdStream::from_raw_socket(self.0) };
            let res = stream.local_addr();
            let _ = stream.into_raw_socket();
            res
        }

        pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
            let stream = unsafe { StdStream::from_raw_socket(self.0) };
            let res = stream.peer_addr();
            let _ = stream.into_raw_socket();
            res
        }
    }
}

#[derive(Debug)]
pub struct TcpStreamRecv {
    inner: BiLock<TcpStream>,
    /// We keep the stream's raw fd aside to be able to get
    /// the local and peer addrs without locking the BiLock.
    /// Locking is not only unnecessary, but also would require
    /// the getters to be async (and likewise on Connection).
    fd: raw_fd::Fd,
}

unsafe impl Send for TcpStreamRecv {}
unsafe impl Sync for TcpStreamRecv {}

fn would_block() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "would block")
}

fn wrap_as_io<T>(t: Async<T>) -> Result<Async<T>, io::Error> {
    Ok(t)
}

impl TcpStreamRecv {
    pub fn shutdown(&self, how: Shutdown) -> Result<(), io::Error> {
        self.fd.shutdown(how)
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.fd.local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.fd.peer_addr()
    }
}

impl io::Read for TcpStreamRecv {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.poll_lock() {
            Async::Ready(mut l) => l.read(buf),
            Async::NotReady => Err(would_block()),
        }
    }
}

impl AsyncRead for TcpStreamRecv {
    fn read_buf<B: bytes::BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        let mut locker = try_ready!(wrap_as_io(self.inner.poll_lock()));
        locker.read_buf(buf)
    }
}

#[derive(Debug)]
pub struct TcpStreamSend {
    inner: BiLock<TcpStream>,
    /// This is the stream's raw fd, refer to doc in `TcpStreamRecv`.
    fd: raw_fd::Fd,
}

unsafe impl Send for TcpStreamSend {}
unsafe impl Sync for TcpStreamSend {}

impl TcpStreamSend {
    pub fn shutdown(&self, how: Shutdown) -> Result<(), io::Error> {
        self.fd.shutdown(how)
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.fd.local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.fd.peer_addr()
    }
}

impl io::Write for TcpStreamSend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.poll_lock() {
            Async::Ready(mut l) => l.write(buf),
            Async::NotReady => Err(would_block()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.inner.poll_lock() {
            Async::Ready(mut l) => l.flush(),
            Async::NotReady => Err(would_block()),
        }
    }
}

impl AsyncWrite for TcpStreamSend {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        let mut locker = try_ready!(wrap_as_io(self.inner.poll_lock()));
        (&mut *locker as &mut AsyncWrite).shutdown()
    }

    fn write_buf<B: bytes::Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        let mut locker = try_ready!(wrap_as_io(self.inner.poll_lock()));
        locker.write_buf(buf)
    }
}

/// This function as well as the split wrappers `TcpStreamSend` and `TcpStreamRecv`
/// exist as a workaround for two problems in Tokio:
///
///  1. Tokio split wrappers (the ones creates by `Stream::split()` as well as `AsyncRead::split()`)
///     provide no access to the underlying stream, ie. there's no way to shutdown
///     the receiving part from the sending part and vice versa.
///     (`AsyncWrite` has `shutdown()`, but it won't shut down the other half.)
///
///     The custom split wrappers provide a custom `shutdown()` method
///     that can be used to shutdown the whole connection.
///
///  2. Tokio split wrappers use locking internally (using `BiLock`)
///     which should not be necessary for a TCP connection.
///     Note: For now we're using BiLock as well until this is fixed in Tokio.
///
/// Cf. [tokio bug #174](https://github.com/tokio-rs/tokio/issues/174)
pub fn tcp_split(stream: TcpStream) -> (TcpStreamSend, TcpStreamRecv) {
    let fd = raw_fd::Fd::from(&stream);
    let (inner1, inner2) = BiLock::new(stream);
    let send = TcpStreamSend { inner: inner1, fd };
    let recv = TcpStreamRecv { inner: inner2, fd };
    (send, recv)
}
