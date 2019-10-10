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

//! Reader/Writer split support for fully duplex sockets.
//!
//! While Tokio does support some form of splitting, it's not sufficient
//! for our usecase because of several reasons:
//!
//!  1. Tokio split wrappers (both the generic ones creates by `tokio::io::split()`
//!     as well as the TCP-specific ones created by `TcpStream::split()`)
//!     provide no access to the underlying stream, ie. there's no way to shutdown
//!     the receiving part from the sending part and vice versa.
//!
//!     Our custom split wrappers provide a custom `shutdown()` method
//!     that can be used to shutdown the whole connection.
//!
//!  2. Tokio's generic I/O split uses an Arc + locking to make sure the access is safe.
//!     This shouldn't be needed for a fully duplex socket.
//!     The TCP-specific split "solves" this problem by basically being a reference
//!     to the original stream, unfortunatelly this makes it impossible to use
//!     the halves independently.
//!
//!     Our implementation currently uses internally the locked generics split wrappers.
//!     This is not ideal and should be revisited in the future...
//!
//! The problem was originally discussed in [tokio bug #174](https://github.com/tokio-rs/tokio/issues/174)
//! w.r.t. Tokio 0.1.X and again at [PR 1521](https://github.com/tokio-rs/tokio/pull/1521) w.r.t. Tokio 0.2.0 alpha.
//!

use std::net::{Shutdown, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};

use ii_async_compat::{tokio, tokio_io};
use pin_project::pin_project;
use tokio::io;
use tokio::net::TcpStream;
use tokio::prelude::{AsyncRead, AsyncWrite};
use tokio_io::split::{ReadHalf, WriteHalf};

/// This is a newtype uniting unix `RawFd` and windows `RawSocket`,
/// implementing local & peer addr getters for use in `TcpDuplexRecv` and `TcpDuplexSend`.
#[cfg(target_family = "unix")]
mod raw_fd {
    use ii_async_compat::tokio::net::TcpStream as TokioStream;
    use std::io;
    use std::net::TcpStream as StdStream;
    use std::net::{Shutdown, SocketAddr};
    use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

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
            let _ = stream.into_raw_fd();
            res
        }
    }
}

#[cfg(target_family = "windows")]
mod raw_fd {
    use ii_async_compat::tokio::net::TcpStream as TokioStream;
    use std::io;
    use std::net::TcpStream as StdStream;
    use std::net::{Shutdown, SocketAddr};
    use std::os::windows::io::{AsRawSocket, IntoRawSocket, RawSocket};

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

/// A trait implementable by socket types that support fully duplex
/// I/O without locking.
pub trait DuplexSplit {
    /// Type of the receiving half
    type DuplexRecv: AsyncRead;
    /// Type of the sending half
    type DuplexSend: AsyncWrite;

    /// Consumes the socket and creates receiving and sending halves from it
    fn duplex_split(self) -> (Self::DuplexRecv, Self::DuplexSend);
}

/// TCP socket receiver half
#[pin_project]
#[derive(Debug)]
pub struct TcpDuplexRecv {
    #[pin]
    inner: ReadHalf<TcpStream>,
    fd: raw_fd::Fd,
}

/// TCP socket sender half
#[pin_project]
#[derive(Debug)]
pub struct TcpDuplexSend {
    #[pin]
    inner: WriteHalf<TcpStream>,
    fd: raw_fd::Fd,
}

impl TcpDuplexRecv {
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

impl TcpDuplexSend {
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

impl AsyncRead for TcpDuplexRecv {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl AsyncWrite for TcpDuplexSend {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }
}

impl DuplexSplit for TcpStream {
    type DuplexRecv = TcpDuplexRecv;
    type DuplexSend = TcpDuplexSend;

    fn duplex_split(self) -> (TcpDuplexRecv, TcpDuplexSend) {
        let fd: raw_fd::Fd = (&self).into();
        let (read, write) = io::split(self);

        let recv = TcpDuplexRecv { inner: read, fd };

        let send = TcpDuplexSend { inner: write, fd };

        (recv, send)
    }
}
