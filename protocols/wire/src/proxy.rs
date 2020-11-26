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

use bytes::Buf;
use bytes::BytesMut;
use futures::{Future, FutureExt};
use pin_project::pin_project;
use std::convert::TryInto;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed, FramedParts};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::connection::Connection;
use crate::framing::Framing;
use codec::{v1::V1Codec, v2::V2Codec, MAX_HEADER_SIZE};
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

    fn proxy_info(&self) -> Result<ProxyInfo> {
        use std::convert::TryFrom;
        let original_source = self.original_peer_addr();
        let original_destination = self.original_destination_addr();
        ProxyInfo::try_from((original_source, original_destination))
    }
}

impl WithProxyInfo for TcpStream {}

pub trait ProxyInfoVisitor {
    fn accept<T: WithProxyInfo>(&mut self, connection_context: &T);
}

impl ProxyInfoVisitor for () {
    fn accept<T: WithProxyInfo>(&mut self, _connection_context: &T) {}
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ProtocolVersion {
    V1,
    V2,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProtocolConfig {
    /// When true, the PROXY protocol is enforced and the server will not accept any connection
    /// that doesn't initiate PROXY protocol session
    pub require_proxy_header: bool,
    /// Accepted versions of PROXY protocol on incoming connection
    pub versions: Vec<ProtocolVersion>,
}

impl ProtocolConfig {
    pub fn new(require_proxy_header: bool, versions: Vec<ProtocolVersion>) -> Self {
        Self {
            require_proxy_header,
            versions,
        }
    }
}

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
    /// When auto-detecting the Proxy protocol header, this is the sufficient number of bytes that
    /// need to be initially received to decide whether any of the supported protocol variants
    const COMMON_HEADER_PREFIX_LEN: usize = 5;

    /// Processes proxy protocol header and creates [`ProxyStream`]
    /// with appropriate information in it
    /// This method may block for ~2 secs until stream timeout is triggered when performing
    /// autodetection and waiting for `COMMON_HEADER_PREFIX_LEN` bytes to arrive.
    pub async fn accept<T: AsyncRead + Send + Unpin>(
        self,
        mut stream: T,
    ) -> Result<ProxyStream<T>> {
        trace!("wire: Accepting stream");
        let mut buf = BytesMut::with_capacity(MAX_HEADER_SIZE);
        // This loop will block for ~2 seconds (read_buf() timeout) if less than
        // COMMON_HEADER_PREFIX_LEN have arrived
        while buf.len() < Self::COMMON_HEADER_PREFIX_LEN {
            let r = stream.read_buf(&mut buf).await?;
            trace!("wire: Read {} bytes from stream", r);
            if r == 0 {
                trace!("wire: no more bytes supplied byte the stream, terminating read");
                break;
            }
        }

        if buf.remaining() < Self::COMMON_HEADER_PREFIX_LEN {
            return if self.require_proxy_header {
                Err(Error::Proxy(
                    "Message too short for autodetecting proxy protocol".into(),
                ))
            } else {
                debug!("wire: No proxy protocol detected (too short message), passing the stream");
                trace!(
                    "wire: Preparing dummy proxy stream with preloaded buffer: {:x?}",
                    buf
                );
                Ok(ProxyStream {
                    inner: stream,
                    buf,
                    orig_source: None,
                    orig_destination: None,
                })
            };
        }
        debug!("wire: Buffered initial {} bytes", buf.remaining());

        if buf[0..Self::COMMON_HEADER_PREFIX_LEN] == V1_TAG[0..Self::COMMON_HEADER_PREFIX_LEN]
            && self.support_v1
        {
            debug!("wire: Detected proxy protocol v1 tag");
            Acceptor::decode_header(buf, stream, V1Codec::new()).await
        } else if buf[0..Self::COMMON_HEADER_PREFIX_LEN]
            == V2_TAG[0..Self::COMMON_HEADER_PREFIX_LEN]
            && self.support_v2
        {
            debug!("wire: Detected proxy protocol v2 tag");
            Acceptor::decode_header(buf, stream, V2Codec::new()).await
        } else if self.require_proxy_header {
            error!("Proxy protocol is required");
            Err(Error::Proxy("Proxy protocol is required".into()))
        } else {
            debug!("wire: No proxy protocol detected, just passing the stream");
            Ok(ProxyStream {
                inner: stream,
                buf,
                orig_source: None,
                orig_destination: None,
            })
        }
    }

    async fn decode_header<C, T>(buf: BytesMut, stream: T, codec: C) -> Result<ProxyStream<T>>
    where
        T: AsyncRead + Unpin,
        C: Encoder<ProxyInfo> + Decoder<Item = ProxyInfo, Error = Error>,
    {
        let mut framed_parts = FramedParts::new(stream, codec);
        framed_parts.read_buf = buf;
        let mut framed = Framed::from_parts(framed_parts);

        let proxy_info = framed
            .next()
            .await
            .ok_or_else(|| Error::Proxy("Proxy header is missing".into()))??;

        let parts = framed.into_parts();

        Ok(ProxyStream {
            inner: parts.io,
            buf: parts.read_buf,
            orig_source: proxy_info.original_source,
            orig_destination: proxy_info.original_destination,
        })
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

/// Represent a prepared acceptor for processing incoming bytes
pub type AcceptorFuture<T> = Box<dyn Future<Output = Result<ProxyStream<T>>> + Send + Unpin>;

/// Internal builder method selected based on configuration used when constructing `AcceptorBuilder`
type BuildMethod<T> = fn(&AcceptorBuilder<T>, T) -> AcceptorFuture<T>;

/// Builder is carries configuration for a future acceptor and is preconfigured early
/// to build an Acceptor in suitable state
#[derive(Clone)]
pub struct AcceptorBuilder<T> {
    config: ProtocolConfig,
    /// Build method for a particular Acceptor variant is selected based on provided configuration
    build_method: BuildMethod<T>,
}

impl<T> AcceptorBuilder<T>
where
    T: AsyncRead + Send + Unpin + 'static,
{
    pub fn new(config: ProtocolConfig) -> Self {
        // TODO for now, we only provide hardcoded autodetect build method
        let build_method = Self::build_auto;
        Self {
            config,
            build_method,
        }
    }

    pub fn build(&self, stream: T) -> AcceptorFuture<T> {
        (self.build_method)(self, stream)
    }

    /// TODO refactor once the Acceptor is streamlined and doesn't require such complex building
    pub fn build_auto(&self, stream: T) -> AcceptorFuture<T> {
        let acceptor = Acceptor::new()
            .support_v1(self.config.versions.contains(&ProtocolVersion::V1))
            .support_v2(self.config.versions.contains(&ProtocolVersion::V2))
            .require_proxy_header(self.config.require_proxy_header);

        Box::new(acceptor.accept(stream).boxed())
    }
}

/// `Connector` enables to add PROXY protocol header to outgoing stream
pub struct Connector {
    protocol_version: ProtocolVersion,
}

impl Connector {
    /// If `use_v2` is true, v2 header will be added
    pub fn new(protocol_version: ProtocolVersion) -> Self {
        Connector { protocol_version }
    }

    /// Creates outgoing TCP connection with appropriate PROXY protocol header
    pub async fn connect(
        &self,
        addr: crate::Address,
        original_source: Option<SocketAddr>,
        original_destination: Option<SocketAddr>,
    ) -> Result<TcpStream> {
        let mut stream = TcpStream::connect(addr.as_ref()).await?;
        self.write_proxy_header(&mut stream, original_source, original_destination)
            .await?;
        Ok(stream)
    }

    /// Adds appropriate PROXY protocol header to given stream
    pub async fn write_proxy_header<T: AsyncWrite + Unpin>(
        &self,
        dest: &mut T,
        original_source: Option<SocketAddr>,
        original_destination: Option<SocketAddr>,
    ) -> Result<()> {
        let proxy_info = (original_source, original_destination).try_into()?;
        let mut data = BytesMut::new();
        match self.protocol_version {
            ProtocolVersion::V1 => V1Codec::new().encode(proxy_info, &mut data)?,
            ProtocolVersion::V2 => V2Codec::new().encode(proxy_info, &mut data)?,
        }

        dest.write(&data).await?;
        Ok(())
    }
}

/// Stream containing information from PROXY protocol
///
#[pin_project]
#[derive(Debug)]
pub struct ProxyStream<T> {
    #[pin]
    inner: T,
    buf: BytesMut,
    orig_source: Option<SocketAddr>,
    orig_destination: Option<SocketAddr>,
}

impl<T> ProxyStream<T> {
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

impl<T: AsyncRead + Send + Unpin> ProxyStream<T> {
    pub async fn new(stream: T) -> Result<Self> {
        Acceptor::default().accept(stream).await
    }
}

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
    use tokio::stream::StreamExt;

    /// Test codec for verifying that a message flows correctly through ProxyStream
    struct TestCodec {
        /// Test message that
        test_message: Vec<u8>,
        buf: BytesMut,
    }

    impl TestCodec {
        pub fn new(test_message: Vec<u8>) -> Self {
            let test_message_len = test_message.len();
            Self {
                test_message,
                buf: BytesMut::with_capacity(test_message_len),
            }
        }
    }

    impl Decoder for TestCodec {
        type Item = Vec<u8>;
        type Error = Error;

        fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>> {
            let received = buf.split();
            self.buf.unsplit(received);
            if self.buf.len() == self.test_message.len() {
                let message_bytes = self.buf.split();
                let item: Vec<u8> = message_bytes[..].into();
                Ok(Some(item))
            } else {
                Ok(None)
            }
        }
    }

    impl Encoder<Vec<u8>> for TestCodec {
        type Error = Error;
        fn encode(&mut self, _item: Vec<u8>, _header: &mut BytesMut) -> Result<()> {
            Err(Error::Proxy("Encoding not to be tested".into()))
        }
    }

    /// Helper that
    async fn read_and_compare_message<T: AsyncRead + Unpin>(
        proxy_stream: ProxyStream<T>,
        test_message: Vec<u8>,
    ) {
        let mut framed_parts =
            FramedParts::new(proxy_stream.inner, TestCodec::new(test_message.clone()));
        framed_parts.read_buf = proxy_stream.buf;
        let mut framed = Framed::from_parts(framed_parts);

        let passed_message = framed
            .next()
            .await
            .expect("BUG: Unexpected end of stream")
            .expect("BUG: Failed to read message from the stream");
        assert_eq!(
            passed_message, test_message,
            "BUG: Message didn't flow successfully"
        );
    }

    #[tokio::test]
    async fn test_v1_tcp4() {
        const HELLO: &'static [u8] = b"HELLO";
        let message = "PROXY TCP4 192.168.0.1 192.168.0.11 56324 443\r\nHELLO".as_bytes();
        let ps = Acceptor::new()
            .accept(message)
            .await
            .expect("BUG: Cannot accept message");
        assert_eq!(
            "192.168.0.1:56324"
                .parse::<SocketAddr>()
                .expect("BUG: Cannot parse IP"),
            ps.original_peer_addr()
                .expect("BUG: Cannot parse original peer IP")
        );
        assert_eq!(
            "192.168.0.11:443"
                .parse::<SocketAddr>()
                .expect("BUG: Cannot parse IP"),
            ps.original_destination_addr()
                .expect("BUG: Cannot parse original dest IP")
        );
        read_and_compare_message(ps, Vec::from(HELLO)).await;
    }

    #[tokio::test]
    async fn test_v2tcp4() {
        let mut message = Vec::new();
        message.extend_from_slice(V2_TAG);
        message.extend(&[
            0x21, 0x11, 0, 12, 192, 168, 0, 1, 192, 168, 0, 11, 0xdc, 0x04, 1, 187,
        ]);
        message.extend(b"Hello");

        let ps = Acceptor::new()
            .accept(&message[..])
            .await
            .expect("BUG: V2 message not accepted");
        assert_eq!(
            "192.168.0.1:56324"
                .parse::<SocketAddr>()
                .expect("BUG: Cannot parse IP"),
            ps.original_peer_addr()
                .expect("BUG: Cannot parse original peer IP")
        );
        assert_eq!(
            "192.168.0.11:443"
                .parse::<SocketAddr>()
                .expect("BUG: Cannot parse IP"),
            ps.original_destination_addr()
                .expect("BUG: Cannot parse original dest IP")
        );
        assert_eq!(
            b"Hello",
            &ps.buf[..],
            "BUG: Expected message not stored in ProxyStream"
        );
    }

    #[tokio::test]
    async fn test_v1_unknown_long_message() {
        let mut message = "PROXY UNKNOWN\r\n".to_string();
        //const DATA_LENGTH: usize = 1_000_000;
        const DATA_LENGTH: usize = 3;
        let data: Vec<u8> = (b'A'..=b'Z').cycle().take(DATA_LENGTH).collect();

        let data_str = String::from_utf8(data.clone()).expect("BUG: cannot build test large data");
        message.push_str(data_str.as_str());

        let ps = ProxyStream::new(message.as_bytes())
            .await
            .expect("BUG: cannot create ProxyStream");
        read_and_compare_message(ps, Vec::from(data)).await;
    }

    #[tokio::test]
    async fn test_no_proxy_header_passed() {
        const MESSAGE: &'static [u8] = b"MEMAM PROXY HEADER, CHUDACEK JA";

        let ps = ProxyStream::new(&MESSAGE[..])
            .await
            .expect("BUG: cannot create ProxyStream");
        assert!(ps.original_peer_addr().is_none());
        assert!(ps.original_destination_addr().is_none());
        read_and_compare_message(ps, Vec::from(MESSAGE)).await;
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
    async fn test_too_short_message_pass() {
        const MESSAGE: &'static [u8] = b"NIC\r\n";
        let ps = Acceptor::new()
            .require_proxy_header(false)
            .accept(&MESSAGE[..])
            .await
            .expect("BUG: Cannot accept message");
        read_and_compare_message(ps, Vec::from(MESSAGE)).await;
    }

    /// Verify that a test message is succesfully passed through the Acceptor and leaves it
    /// untouched in the form of ProxyStream with prepared buffer. We use framed with a test
    /// codec to actually collect the message again
    #[tokio::test]
    async fn test_short_message_retention_via_proxy_stream() {
        const MESSAGE: &'static [u8] = b"NIC\r\n";
        let ps = Acceptor::new()
            .require_proxy_header(false)
            .accept(&MESSAGE[..])
            .await
            .expect("BUG: Cannot accept incoming message");

        read_and_compare_message(ps, Vec::from(MESSAGE)).await;
    }

    #[tokio::test]
    async fn test_connect() {
        let mut buf = Vec::new();
        let src = "127.0.0.1:1111"
            .parse::<SocketAddr>()
            .expect("BUG: Cannot parse IP");
        let dest = "127.0.0.1:2222"
            .parse::<SocketAddr>()
            .expect("BUG: Cannot parse IP");
        let _res = Connector::new(ProtocolVersion::V1)
            .write_proxy_header(&mut buf, Some(src), Some(dest))
            .await
            .expect("BUG: Cannot write proxy header");
        let expected = "PROXY TCP4 127.0.0.1 127.0.0.1 1111 2222\r\n";
        assert_eq!(expected.as_bytes(), &buf[..]);
    }

    /// Verify that build_auto method has been detected
    #[test]
    fn acceptor_builder_auto() {
        let acceptor_builder: AcceptorBuilder<&[u8]> = AcceptorBuilder::new(ProtocolConfig::new(
            false,
            vec![ProtocolVersion::V1, ProtocolVersion::V2],
        ));

        let actual = acceptor_builder.build_method as *const BuildMethod<&[u8]>;
        let expected = AcceptorBuilder::<&[u8]>::build_auto as *const BuildMethod<&[u8]>;

        assert_eq!(actual, expected, "BUG: Expected auto method");
    }
}
