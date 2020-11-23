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

use super::{ProxyInfo, SocketType};
use crate::proxy::error::{Error, Result};
use bytes::BytesMut;
use futures::stream::StreamExt;
use proto::*;
use std::net::SocketAddr;
use tokio::prelude::*;
use tokio_util::codec::{Decoder, Encoder, Framed, FramedParts};

pub mod proto;

pub const SIGNATURE: &[u8] = b"\x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A";

pub struct V2Codec {
    socket_type: Option<SocketType>,
    remains: usize,
}

impl Default for V2Codec {
    fn default() -> Self {
        V2Codec {
            socket_type: None,
            remains: 0,
        }
    }
}

impl V2Codec {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Decoder for V2Codec {
    type Item = ProxyInfo;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>> {
        loop {
            match self.socket_type {
                Some(t) => {
                    if self.remains > 0 && buf.len() < self.remains {
                        return Ok(None);
                    } else {
                        let mut data_buf = buf.split_to(self.remains);
                        let info = match t {
                            SocketType::Ipv4 => {
                                let addresses = Ip4Addresses::deserialize(&mut data_buf)?;
                                let (src, dst) = addresses.into();
                                ProxyInfo {
                                    socket_type: t,
                                    original_source: Some(SocketAddr::V4(src)),
                                    original_destination: Some(SocketAddr::V4(dst)),
                                }
                            }
                            SocketType::Ipv6 => {
                                let addresses = Ip6Addresses::deserialize(&mut data_buf)?;
                                let (src, dst) = addresses.into();
                                ProxyInfo {
                                    socket_type: t,
                                    original_source: Some(SocketAddr::V6(src)),
                                    original_destination: Some(SocketAddr::V6(dst)),
                                }
                            }
                            SocketType::Unknown => ProxyInfo {
                                socket_type: t,
                                original_source: None,
                                original_destination: None,
                            },
                        };
                        self.socket_type = None;
                        self.remains = 0;
                        return Ok(Some(info));
                    }
                }
                None => {
                    if buf.len() < SIZE_HEADER as usize {
                        return Ok(None);
                    } else {
                        let header = Header::deserialize(buf)?;
                        self.remains = header.len as usize;
                        match header.protocol {
                            PROTOCOL_TCP_IP4 => self.socket_type = Some(SocketType::Ipv4),
                            PROTOCOL_TCP_IP6 => self.socket_type = Some(SocketType::Ipv6),
                            p => {
                                warn!("Yet unsupported protocol, code {}", p);
                                self.socket_type = Some(SocketType::Unknown);
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Encoder<ProxyInfo> for V2Codec {
    type Error = Error;
    fn encode(&mut self, item: ProxyInfo, buf: &mut BytesMut) -> Result<()> {
        let header = Header::new(item.socket_type);
        header.serialize(buf);
        match item.socket_type {
            SocketType::Ipv4 => {
                if let (Some(SocketAddr::V4(src)), Some(SocketAddr::V4(dst))) =
                    (item.original_source, item.original_destination)
                {
                    let addresses: Ip4Addresses = (src, dst).into();
                    addresses.serialize(buf);
                } else {
                    return Err(Error::Proxy("Both V4 addresses must be present".into()));
                }
            }

            SocketType::Ipv6 => {
                if let (Some(SocketAddr::V6(src)), Some(SocketAddr::V6(dst))) =
                    (item.original_source, item.original_destination)
                {
                    let addresses: Ip6Addresses = (src, dst).into();
                    addresses.serialize(buf);
                } else {
                    return Err(Error::Proxy("Both V4 addresses must be present".into()));
                }
            }
            SocketType::Unknown => (),
        }

        Ok(())
    }
}

/// Helper function to accept stream with PROXY protocol header v2
///
/// Consumes header and returns appropriate `ProxyInfo` and rest of data as `FramedParts`,
/// which can be used to easily create new Framed struct (with different codec)
pub async fn accept_v2_framed<T>(stream: T) -> Result<(ProxyInfo, FramedParts<T, V2Codec>)>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let mut framed = Framed::new(stream, V2Codec::new());
    let proxy_info = framed
        .next()
        .await
        .ok_or_else(|| Error::Proxy("Proxy header is missing".into()))??;
    let parts = framed.into_parts();
    Ok((proxy_info, parts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    fn test_msg_ip4(msg: &str) -> BytesMut {
        let mut output = BytesMut::with_capacity(16 + 12 + msg.len());
        output.extend_from_slice(SIGNATURE);
        output.put_u8(0x21);
        output.put_u8(0x11);
        output.extend(&[0, 12]);
        output.extend(&[127, 0, 0, 1]);
        output.extend(&[127, 0, 0, 2]);
        output.extend(&[0, 80]);
        output.extend(&[1, 187]);
        output.extend(msg.as_bytes());
        output
    }

    #[test]
    fn test_v2_proxy_decode() {
        let mut buf = test_msg_ip4("Hello");
        let mut codec = V2Codec::new();
        let info = codec
            .decode(&mut buf)
            .expect("BUG: ProxyInfo not decoded")
            .expect("BUG: ProxyInfo decoding faile");
        let src_addr: SocketAddr = "127.0.0.1:80".parse().expect("BUG: Cannot parse src IP");
        let dst_addr: SocketAddr = "127.0.0.2:443".parse().expect("BUG: Cannot parse dst IP");

        assert_eq!(Some(src_addr), info.original_source);
        assert_eq!(Some(dst_addr), info.original_destination);
        assert_eq!(5, buf.len());
    }

    #[tokio::test]
    async fn test_accept_v2_framed() {
        let buf = std::io::Cursor::new(test_msg_ip4("Hello").to_vec());
        let (info, parts) = accept_v2_framed(buf).await.expect("BUG: parses ok");
        let src_addr: SocketAddr = "127.0.0.1:80".parse().expect("BUG: Cannot parse src IP");
        let dst_addr: SocketAddr = "127.0.0.2:443".parse().expect("BUG: Cannot parse dst IP");
        assert_eq!(Some(src_addr), info.original_source);
        assert_eq!(Some(dst_addr), info.original_destination);
        assert_eq!(5, parts.read_buf.len());
    }

    #[test]
    fn test_v2_encode() {
        let src_addr: SocketAddr = "127.0.0.1:80".parse().expect("BUG: Cannot parse src IP");
        let dst_addr: SocketAddr = "127.0.0.2:443".parse().expect("BUG: Cannot parse dst IP");
        let info = ProxyInfo {
            socket_type: SocketType::Ipv4,
            original_source: Some(src_addr),
            original_destination: Some(dst_addr),
        };
        let mut buf = BytesMut::new();
        let mut codec = V2Codec::new();
        codec
            .encode(info.clone(), &mut buf)
            .expect("BUG: encoding failed");
        let info2 = codec
            .decode(&mut buf)
            .expect("BUG: No ProxyInfo decoded")
            .expect("BUG: ProxyInfo decoding failed");
        assert_eq!(info, info2);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_v2_ip6_encode_decode() {
        let src_addr: SocketAddr = "[ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:80"
            .parse()
            .expect("BUG: Cannot parse src IPv6");
        let dst_addr: SocketAddr = "[aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa]:443"
            .parse()
            .expect("BUG: Cannot parse dst IPv6");
        let info = ProxyInfo {
            socket_type: SocketType::Ipv6,
            original_source: Some(src_addr),
            original_destination: Some(dst_addr),
        };
        let mut buf = BytesMut::new();
        let mut codec = V2Codec::new();
        codec.encode(info.clone(), &mut buf).expect("BUG: encoding");
        let info2 = codec
            .decode(&mut buf)
            .expect("BUG: No ProxyInfo decoded")
            .expect("BUG: ProxInfo decoding failed");
        assert_eq!(info, info2);
        assert!(buf.is_empty());
    }
}
