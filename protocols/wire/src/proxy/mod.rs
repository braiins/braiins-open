// Copyright (C) 2020  Braiins Systems s.r.o.
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

//! Implements  [PROXY protocol](http://www.haproxy.org/download/1.8/doc/proxy-protocol.txt) in tokio
//!
//! TODO: currently only v1 is implemented

use bytes::Buf;
use bytes::BytesMut;
use pin_project::pin_project;
use std::convert::TryInto;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;

use tokio::prelude::*;
use tokio_util::codec::{Decoder, Encoder, Framed, FramedParts};

use crate::connection::Connection;
use crate::framing::Framing;
use codec::{v1::V1Codec, v2::V2Codec, MAX_HEADER_SIZE, MIN_HEADER_SIZE};
use error::{Error, Result};

pub mod codec;
pub mod error;
pub use codec::{v1::accept_v1_framed, v2::accept_v2_framed, ProxyInfo};

const V1_TAG: &[u8] = b"PROXY ";
const V2_TAG: &[u8] = codec::v2::SIGNATURE;

/// Information from proxy protocol are provided through `WithProxyInfo` trait,
/// which provides original addresses from PROXY protocol
///
/// For compatibility it's also implemented for `tokio::net::TcpSteam`, where it returns `None`
pub trait WithProxyInfo {
    // TODO: or original_source_addr - which one is better?
    /// Returns address of original source of the connection (client)
    fn original_peer_addr(&self) -> Option<SocketAddr> {
        None
    }

    /// Returns address of original destination of the connection (e.g. first proxy)
    fn original_destination_addr(&self) -> Option<SocketAddr> {
        None
    }
}

impl WithProxyInfo for TcpStream {}

/// Struct to accept stream with PROXY header and extract information from it
pub struct Acceptor {
    require_proxy_header: bool,
    support_v1: bool,
    support_v2: bool,
}

impl Default for Acceptor {
    fn default() -> Self {
        Acceptor {
            require_proxy_header: false,
            support_v1: true,
            support_v2: true,
        }
    }
}

impl Acceptor {
    /// Processes proxy protocol header and creates [`ProxyStream`]
    /// with appropriate information in it
    pub async fn accept<T: AsyncRead + Unpin>(self, mut stream: T) -> Result<ProxyStream<T>> {
        let mut buf = BytesMut::with_capacity(MAX_HEADER_SIZE);
        while buf.len() < MIN_HEADER_SIZE {
            let r = stream.read_buf(&mut buf).await?;
            if r == 0 {
                break;
            }
        }

        if buf.remaining() < MIN_HEADER_SIZE {
            return if self.require_proxy_header {
                Err(Error::Proxy("Message too short for proxy protocol".into()))
            } else {
                info!("No proxy protocol detected (because of too short message),  just passing the stream");
                Ok(ProxyStream {
                    inner: stream,
                    buf,
                    orig_source: None,
                    orig_destination: None,
                })
            };
        }

        debug!("Buffered initial {} bytes", buf.remaining());

        if &buf[0..6] == V1_TAG && self.support_v1 {
            debug!("Detected proxy protocol v1 tag");
            let mut codec = V1Codec::new();
            Acceptor::decode_header(buf, stream, &mut codec).await
        } else if &buf[0..12] == V2_TAG && self.support_v2 {
            debug!("Detected proxy protocol v2 tag");
            let mut codec = V2Codec::new();
            Acceptor::decode_header(buf, stream, &mut codec).await
        } else if self.require_proxy_header {
            error!("Proxy protocol is required");
            Err(Error::Proxy("Proxy protocol is required".into()))
        } else {
            info!("No proxy protocol detected, just passing the stream");
            Ok(ProxyStream {
                inner: stream,
                buf,
                orig_source: None,
                orig_destination: None,
            })
        }
    }

    async fn decode_header<C, T>(
        mut buf: BytesMut,
        mut stream: T,
        codec: &mut C,
    ) -> Result<ProxyStream<T>>
    where
        T: AsyncRead + Unpin,
        C: Decoder<Item = ProxyInfo, Error = Error>,
    {
        loop {
            if let Some(proxy_info) = codec.decode(&mut buf)? {
                return Ok(ProxyStream {
                    inner: stream,
                    buf,
                    orig_source: proxy_info.original_source,
                    orig_destination: proxy_info.original_destination,
                });
            }

            let r = stream.read_buf(&mut buf).await?;
            if r == 0 {
                return Err(Error::Proxy("Incomplete V1 header".into()));
            }
        }
    }

    /// Creates new default `Acceptor`
    pub fn new() -> Self {
        Acceptor::default()
    }

    /// If true (default) PROXY header is required in accepted stream, if not present error is raised.
    /// If set to false, then if PROXY header is not present, stream is created and all data passed on.
    /// Original addresses then are not known indeed.
    pub fn require_proxy_header(self, require_proxy_header: bool) -> Self {
        Acceptor {
            require_proxy_header,
            ..self
        }
    }

    /// If true v1 PROXY protocol is supported (default)
    pub fn support_v1(self, support_v1: bool) -> Self {
        Acceptor { support_v1, ..self }
    }

    /// TODO: Add v2 support
    /// If true v2 PROXY protocol is supported (default)
    pub fn support_v2(self, support_v2: bool) -> Self {
        Acceptor { support_v2, ..self }
    }
}

/// `Connector` enables to add PROXY protocol header to outgoing stream
pub struct Connector {
    use_v2: bool,
}

impl Default for Connector {
    fn default() -> Self {
        Connector { use_v2: false }
    }
}

impl Connector {
    /// Creates new `Connector`
    pub fn new() -> Self {
        Connector::default()
    }

    /// TODO: Add v2 support
    /// If `use_v2` is true, v2 header will be added
    pub fn use_v2(self, use_v2: bool) -> Self {
        Connector { use_v2 }
    }

    /// Creates outgoing TCP connection with appropriate PROXY protocol header
    pub async fn connect(
        &self,
        addr: crate::Address,
        original_source: Option<SocketAddr>,
        original_destination: Option<SocketAddr>,
    ) -> Result<TcpStream> {
        let mut stream = TcpStream::connect(addr.as_ref()).await?;
        self.connect_to(&mut stream, original_source, original_destination)
            .await?;
        Ok(stream)
    }

    /// Adds appropriate PROXY protocol header to given stream
    pub async fn connect_to<T: AsyncWrite + Unpin>(
        &self,
        dest: &mut T,
        original_source: Option<SocketAddr>,
        original_destination: Option<SocketAddr>,
    ) -> Result<()> {
        let proxy_info = (original_source, original_destination).try_into()?;
        let mut data = BytesMut::new();
        if !self.use_v2 {
            V1Codec::new().encode(proxy_info, &mut data)?;
        } else {
            V2Codec::new().encode(proxy_info, &mut data)?
        }
        dest.write(&data).await?;
        Ok(())
    }
}

/// Stream containing information from PROXY protocol
///
/// It implements `AsyncRead` and `AsyncWrite` so it can be used as a replacement of `TcpStream`
/// or other byte streams
#[pin_project]
pub struct ProxyStream<T> {
    #[pin]
    inner: T,
    buf: BytesMut,
    orig_source: Option<SocketAddr>,
    orig_destination: Option<SocketAddr>,
}

impl<T> ProxyStream<T> {
    fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut T> {
        self.project().inner
    }

    /// Returns inner stream, but
    /// only when it is save, e.g. no data in buffer
    pub fn try_into_inner(self) -> Result<T> {
        if self.buf.is_empty() {
            Ok(self.inner)
        } else {
            Err(Error::InvalidState(
                "Cannot return inner steam because buffer is not empty".into(),
            ))
        }
    }
}

impl<T> AsRef<T> for ProxyStream<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

// with Deref we can get automatic coercion for some TcpStream methods

impl std::ops::Deref for ProxyStream<TcpStream> {
    type Target = TcpStream;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// TODO: do we want to allow this - because it can cause problem, if used unwisely
// actually DerefMut mut can be quite dangerous, because it'll enable to inner stream, while some data are already in buffer
// impl std::ops::DerefMut for ProxyStream<TcpStream> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.inner
//     }
// }

// same for AsMut - try to not to use it
// impl<T> AsMut<T> for ProxyStream<T> {
//     fn as_mut(&mut self) -> &mut T {
//         &mut self.inner
//     }
// }

impl<T> WithProxyInfo for ProxyStream<T> {
    fn original_peer_addr(&self) -> Option<SocketAddr> {
        self.orig_source
    }

    fn original_destination_addr(&self) -> Option<SocketAddr> {
        self.orig_destination
    }
}

impl<T: AsyncRead + Unpin> ProxyStream<T> {
    pub async fn new(stream: T) -> Result<Self> {
        Acceptor::default().accept(stream).await
    }
}

// impl<T: AsyncRead + Unpin> AsyncRead for ProxyStream<T> {
//     fn poll_read(
//         self: Pin<&mut Self>,
//         ctx: &mut Context,
//         buf: &mut ReadBuf,
//     ) -> Poll<io::Result<()>> {
//         let this = self.project();
//         if this.buf.is_empty() {
//             this.inner.poll_read(ctx, buf)
//         } else {
//             // send remaining data from buffer
//             let to_copy = this.buf.remaining().min(buf.filled().len());
//             this.buf.copy_to_slice(&mut buf[0..to_copy]);
//
//             //there is still space in output buffer
//             // let's try if we have some bytes to add there
//             if to_copy < buf.len() {
//                 let added = match this.inner.poll_read(ctx, &mut buf[to_copy..]) {
//                     Poll::Ready(Ok(n)) => n,
//                     Poll::Ready(Err(e)) => return Err(e).into(),
//                     Poll::Pending => 0,
//                 };
//                 Poll::Ready(Ok(to_copy + added))
//             } else {
//                 Poll::Ready(Ok(to_copy))
//             }
//         }
//     }
// }
//
// impl<R: AsyncRead + AsyncWrite> AsyncWrite for ProxyStream<R> {
//     fn poll_write(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//         buf: &[u8],
//     ) -> Poll<io::Result<usize>> {
//         self.get_pin_mut().poll_write(cx, buf)
//     }
//
//     fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
//         self.get_pin_mut().poll_flush(cx)
//     }
//
//     fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
//         self.get_pin_mut().poll_shutdown(cx)
//     }
// }

impl<F> From<ProxyStream<TcpStream>> for Connection<F>
where
    F: Framing,
    F::Codec: Default,
{
    fn from(stream: ProxyStream<TcpStream>) -> Self {
        let mut parts = FramedParts::new(stream.inner, F::Codec::default());
        parts.read_buf = stream.buf; // pass existing read buffer
        Connection {
            framed_stream: Framed::from_parts(parts),
        }
    }
}

impl<C> From<ProxyStream<TcpStream>> for Framed<TcpStream, C>
where
    C: Encoder<ProxyInfo> + Decoder + Default,
{
    fn from(stream: ProxyStream<TcpStream>) -> Self {
        let parts = FramedParts::from(stream);
        Framed::from_parts(parts)
    }
}

impl<C> From<ProxyStream<TcpStream>> for FramedParts<TcpStream, C>
where
    C: Encoder<ProxyInfo> + Decoder + Default,
{
    fn from(stream: ProxyStream<TcpStream>) -> Self {
        let mut parts = FramedParts::new(stream.inner, C::default());
        parts.read_buf = stream.buf;
        parts
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_v1_tcp4() -> Result<()> {
        let message = "PROXY TCP4 192.168.0.1 192.168.0.11 56324 443\r\nHELLO".as_bytes();
        let mut ps = Acceptor::new().accept(message).await?;
        assert_eq!(
            "192.168.0.1:56324".parse::<SocketAddr>().unwrap(),
            ps.original_peer_addr().unwrap()
        );
        assert_eq!(
            "192.168.0.11:443".parse::<SocketAddr>().unwrap(),
            ps.original_destination_addr().unwrap()
        );
        let mut buf = Vec::new();
        ps.read_to_end(&mut buf).await?;
        assert_eq!(b"HELLO", &buf[..]);
        Ok(())
    }

    #[tokio::test]
    async fn test_v2tcp4() -> Result<()> {
        let mut message = Vec::new();
        message.extend_from_slice(V2_TAG);
        message.extend(&[
            0x21, 0x11, 0, 12, 192, 168, 0, 1, 192, 168, 0, 11, 0xdc, 0x04, 1, 187,
        ]);
        message.extend(b"Hello");

        let mut ps = Acceptor::new().accept(&message[..]).await?;
        assert_eq!(
            "192.168.0.1:56324".parse::<SocketAddr>().unwrap(),
            ps.original_peer_addr().unwrap()
        );
        assert_eq!(
            "192.168.0.11:443".parse::<SocketAddr>().unwrap(),
            ps.original_destination_addr().unwrap()
        );
        let mut buf = Vec::new();
        ps.read_to_end(&mut buf).await?;
        assert_eq!(b"Hello", &buf[..]);
        Ok(())
    }

    #[tokio::test]
    async fn test_v1_unknown_long_message() -> Result<()> {
        let mut message = "PROXY UNKNOWN\r\n".to_string();
        const DATA_LENGTH: usize = 1_000_000;
        let data = (b'A'..=b'Z').cycle().take(DATA_LENGTH).map(|c| c as char);
        message.extend(data);

        let mut ps = ProxyStream::new(message.as_bytes()).await?;
        assert!(ps.original_peer_addr().is_none());
        assert!(ps.original_destination_addr().is_none());
        let mut buf = Vec::new();
        ps.read_to_end(&mut buf).await?;
        assert_eq!(DATA_LENGTH, buf.len());
        Ok(())
    }

    #[tokio::test]
    async fn test_no_proxy_header_passed() -> Result<()> {
        let message = b"MEMAM PROXY HEADER, CHUDACEK JA";
        let mut ps = ProxyStream::new(&message[..]).await?;
        assert!(ps.original_peer_addr().is_none());
        assert!(ps.original_destination_addr().is_none());
        let mut buf = Vec::new();
        ps.read_to_end(&mut buf).await?;
        assert_eq!(&message[..], &buf[..]);
        Ok(())
    }

    #[tokio::test]
    async fn test_no_proxy_header_rejected() {
        let message = b"MEMAM PROXY HEADER, CHUDACEK JA";
        let ps = Acceptor::new()
            .require_proxy_header(true)
            .accept(&message[..])
            .await;
        assert!(ps.is_err());
    }

    #[tokio::test]
    async fn test_too_short_message_fail() {
        let message = b"NIC\r\n";
        let ps = Acceptor::new()
            .require_proxy_header(true)
            .accept(&message[..])
            .await;
        assert!(ps.is_err());
    }

    #[tokio::test]
    async fn test_too_short_message_pass() -> Result<()> {
        let message = b"NIC\r\n";
        let mut ps = Acceptor::new()
            .require_proxy_header(false)
            .accept(&message[..])
            .await?;
        let mut buf = Vec::new();
        ps.read_to_end(&mut buf).await?;
        assert_eq!(message, &buf[..]);
        Ok(())
    }

    #[tokio::test]
    async fn test_connect() -> Result<()> {
        let mut buf = Vec::new();
        let src = "127.0.0.1:1111".parse().ok();
        let dest = "127.0.0.1:2222".parse().ok();
        let _res = Connector::new().connect_to(&mut buf, src, dest).await?;
        let expected = "PROXY TCP4 127.0.0.1 127.0.0.1 1111 2222\r\n";
        assert_eq!(expected.as_bytes(), &buf[..]);
        Ok(())
    }
}
