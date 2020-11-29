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

use super::{ProxyInfo, SocketType, MAX_HEADER_SIZE};
use crate::proxy::error::{Error, Result};
use bytes::{Buf, BufMut, BytesMut};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use tokio_util::codec::{Decoder, Encoder};

/// Encoder and Decoder for PROXY protocol v1
pub struct V1Codec {
    next_pos: usize,
    pass_header: bool,
}

impl Default for V1Codec {
    fn default() -> Self {
        V1Codec::new()
    }
}

impl V1Codec {
    pub fn new() -> Self {
        V1Codec {
            next_pos: 0,
            pass_header: false,
        }
    }

    pub fn new_with_pass_header(pass_header: bool) -> Self {
        V1Codec {
            next_pos: 0,
            pass_header,
        }
    }
}

fn parse_addresses<T>(parts: &[&str]) -> Result<(SocketAddr, SocketAddr)>
where
    T: FromStr,
    std::net::IpAddr: From<T>,
    Error: From<<T as FromStr>::Err>,
{
    let orig_sender_addr: T = parts[2].parse()?;
    let orig_sender_port: u16 = parts[4].parse::<u16>()?;
    let orig_recipient_addr: T = parts[3].parse()?;
    let orig_recipient_port: u16 = parts[5].parse::<u16>()?;

    Ok((
        (orig_sender_addr, orig_sender_port).into(),
        (orig_recipient_addr, orig_recipient_port).into(),
    ))
}

impl Decoder for V1Codec {
    type Item = ProxyInfo;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>> {
        if let Some(eol_pos) = buf[self.next_pos..].windows(2).position(|w| w == b"\r\n") {
            let eol_pos = eol_pos + self.next_pos;
            let header = std::str::from_utf8(&buf[..eol_pos])?;

            debug!("Proxy header is {}", header);
            let parts: Vec<_> = header.split(' ').collect();
            if parts[0] != "PROXY" {
                return Err(Error::Proxy("Protocol tag is wrong".into()));
            }
            if parts.len() < 2 {
                return Err(Error::Proxy("At least two parts are needed".into()));
            }

            let res = match parts[1] {
                "UNKNOWN" => Ok(Some(ProxyInfo {
                    socket_type: SocketType::Unknown,
                    original_source: None,
                    original_destination: None,
                })),
                "TCP4" if parts.len() == 6 => {
                    let (original_source, original_destination) =
                        parse_addresses::<Ipv4Addr>(&parts)?;
                    if !original_source.is_ipv4() && !original_destination.is_ipv4() {
                        return Err(Error::Proxy("Invalid address version - expected V4".into()));
                    }
                    Ok(Some(ProxyInfo {
                        socket_type: SocketType::Ipv4,
                        original_source: Some(original_source),
                        original_destination: Some(original_destination),
                    }))
                }
                "TCP6" if parts.len() == 6 => {
                    let (original_source, original_destination) =
                        parse_addresses::<Ipv6Addr>(&parts)?;
                    if !original_source.is_ipv6() && !original_destination.is_ipv6() {
                        return Err(Error::Proxy("Invalid address version - expected V6".into()));
                    }
                    Ok(Some(ProxyInfo {
                        socket_type: SocketType::Ipv6,
                        original_source: Some(original_source),
                        original_destination: Some(original_destination),
                    }))
                }
                _ => Err(Error::Proxy(format!("Invalid proxy header v1: {}", header))),
            };

            if !self.pass_header {
                buf.advance(eol_pos + 2);
            }

            res
        } else if buf.len() < MAX_HEADER_SIZE {
            self.next_pos = if buf.is_empty() { 0 } else { buf.len() - 1 };
            Ok(None)
        } else {
            Err(Error::Proxy("Proxy header v1 does not contain EOL".into()))
        }
    }
}

impl Encoder<ProxyInfo> for V1Codec {
    type Error = Error;
    fn encode(&mut self, item: ProxyInfo, header: &mut BytesMut) -> Result<()> {
        header.put(&b"PROXY "[..]);

        let proto = match item {
            ProxyInfo {
                socket_type: SocketType::Ipv4,
                ..
            } => "TCP4",
            ProxyInfo {
                socket_type: SocketType::Ipv6,
                ..
            } => "TCP6",
            ProxyInfo {
                socket_type: SocketType::Unknown,
                ..
            } => {
                header.put(&b"UNKNOWN\r\n"[..]);
                return Ok(());
            }
        };
        let original_source = item.original_source.expect("BUG: Source IP missing");
        let original_destination = item.original_destination.expect("BUG: Source IP missing");
        header.put(
            format!(
                "{} {} {} {} {}\r\n",
                proto,
                original_source.ip(),
                original_destination.ip(),
                original_source.port(),
                original_destination.port()
            )
            .as_bytes(),
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{BufMut, BytesMut};
    use tokio::prelude::*;
    use tokio::stream::StreamExt;
    use tokio_util::codec::{Framed, FramedParts};

    /// Helper function to accept stream with PROXY protocol header v1
    ///
    /// Consumes header and returns appropriate `ProxyInfo` and rest of data as `FramedParts`,
    /// which can be used to easily create new Framed struct (with different codec)
    async fn accept_v1_framed<T>(stream: T) -> Result<(ProxyInfo, FramedParts<T, V1Codec>)>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let mut framed = Framed::new(stream, V1Codec::new());
        let proxy_info = framed
            .next()
            .await
            .ok_or_else(|| Error::Proxy("Proxy header is missing".into()))??;
        let parts = framed.into_parts();
        Ok((proxy_info, parts))
    }

    #[test]
    fn test_header_v1_in_small_pieces() {
        let data = [
            "PROX",
            "Y TCP4",
            " 192.168",
            ".0.1 ",
            "19",
            "2.168.0.1",
            "1 563",
            "24 443\r",
            "\nUsak",
        ];
        let data: Vec<_> = data.iter().map(|s| s.as_bytes()).collect();
        let mut d = V1Codec::new();
        let mut buf = BytesMut::new();
        for &piece in &data[..data.len() - 1] {
            buf.put(piece);
            let r = d.decode(&mut buf).expect("BUG: cannot decode");
            assert!(r.is_none())
        }
        // put there last piece

        buf.put(*data.last().expect("BUG: Last piece of data missing"));
        let r = d
            .decode(&mut buf)
            .expect("BUG: No result from decoding buffer")
            .expect("BUG: Header decoding failed");
        assert_eq!(SocketType::Ipv4, r.socket_type);
        assert_eq!(
            "192.168.0.1:56324"
                .parse::<SocketAddr>()
                .expect("BUG: Cannot parse IP"),
            r.original_source.expect("BUG: Missing original address")
        );
        assert_eq!(
            "192.168.0.11:443"
                .parse::<SocketAddr>()
                .expect("BUG: Cannot parse IP"),
            r.original_destination
                .expect("BUG: Missing destination address")
        );
        assert_eq!(b"Usak", &buf[..]);
    }

    #[test]
    fn test_long_v1_header_without_eol() {
        let data = (b'a'..b'z').cycle().take(600).collect::<Vec<_>>();
        let mut buf = BytesMut::from(&data[..]);
        let mut d = V1Codec::new();
        let r = d.decode(&mut buf);
        assert!(r.is_err());
        if let Err(Error::Proxy(m)) = r {
            assert!(
                m.contains("does not contain EOL"),
                "error is  about missing EOL"
            )
        } else {
            panic!("Wrong error")
        }
    }

    #[test]
    fn test_v1_header_creation() {
        let header_bytes = "PROXY TCP4 192.168.0.1 192.168.0.11 56324 443\r\n".as_bytes();
        let header_info = ProxyInfo {
            socket_type: SocketType::Ipv4,
            original_source: "192.168.0.1:56324".parse().ok(),
            original_destination: "192.168.0.11:443".parse().ok(),
        };

        let mut buf = BytesMut::new();
        let mut e = V1Codec::new();
        e.encode(header_info, &mut buf)
            .expect("BUG: Cannot encode V1 header");

        assert_eq!(header_bytes, &buf[..]);
    }

    #[test]
    fn test_v1_header_creation_for_ipv6() {
        let header_bytes = b"PROXY TCP6 ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa 65535 65534\r\n";
        let header_info = ProxyInfo {
            socket_type: SocketType::Ipv6,
            original_source: "[ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535"
                .parse()
                .ok(),
            original_destination: "[aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa]:65534"
                .parse()
                .ok(),
        };

        let mut buf = BytesMut::new();
        let mut e = V1Codec::new();
        e.encode(header_info, &mut buf)
            .expect("BUG: Cannot encode header info");

        assert_eq!(&header_bytes[..], &buf[..]);
    }

    #[test]
    fn test_v1_header_decode_tcp6() {
        let header_bytes = b"PROXY TCP6 ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa 65535 65534\r\nHello";
        let mut d = V1Codec::new();
        let mut buf = BytesMut::new();
        buf.put(&header_bytes[..]);
        let header_info = d
            .decode(&mut buf)
            .expect("BUG: No header decoded")
            .expect("BUG: Header decoding failed");
        let original_source: Option<SocketAddr> = "[ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535"
            .parse()
            .ok();
        let original_destination: Option<SocketAddr> =
            "[aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa]:65534"
                .parse()
                .ok();
        assert_eq!(SocketType::Ipv6, header_info.socket_type);
        assert_eq!(original_source, header_info.original_source);
        assert_eq!(original_destination, header_info.original_destination);
    }

    #[tokio::test]
    async fn test_accept_v1() {
        use std::io::Cursor;
        let message = Cursor::new(
            "PROXY TCP4 192.168.0.1 192.168.0.11 56324 443\r\nHello"
                .as_bytes()
                .to_vec(),
        );
        let (info, parts) = accept_v1_framed(message)
            .await
            .expect("BUG: Error parsing header");

        assert_eq!(SocketType::Ipv4, info.socket_type);
        assert_eq!(info.original_source, "192.168.0.1:56324".parse().ok());
        assert_eq!(info.original_destination, "192.168.0.11:443".parse().ok());

        assert_eq!(b"Hello", &parts.read_buf[..])
    }

    #[tokio::test]
    async fn test_accept_v1_incomplete_header() {
        use std::io::Cursor;
        let message = Cursor::new("PROXY TCP4 192.168.0.1 192.168.".as_bytes().to_vec());
        let res = accept_v1_framed(message).await;

        assert!(res.is_err());

        if let Err(Error::Io(e)) = res {
            println!("ERROR: {:?}", e);
        } else {
            panic!("Invalid error")
        }
    }

    #[tokio::test]
    async fn test_accept_v1_malformed() {
        use std::io::Cursor;
        let message = Cursor::new(
            "PROXY TCP4 192.168.0.1 192.168.\r\nHello"
                .as_bytes()
                .to_vec(),
        );
        let res = accept_v1_framed(message).await;

        assert!(res.is_err());

        if let Err(Error::Proxy(e)) = res {
            println!("ERROR: {}", e);
        } else {
            panic!("Invalid error")
        }
    }
}
