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

#![allow(dead_code)]

use std::net::{SocketAddrV4, SocketAddrV6};

use super::{SocketType, SIGNATURE};

use crate::bytes;

use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;

// PROXY Protocol version
// As of this specification, it must
// always be sent as \x2 and the receiver must only accept this value.

const PROXY_VERSION: u8 = 0x2;

// Commands

// \x0 : LOCAL : the connection was established on purpose by the proxy
// without being relayed. The connection endpoints are the sender and the
// receiver. Such connections exist when the proxy sends health-checks to the
// server. The receiver must accept this connection as valid and must use the
// real connection endpoints and discard the protocol block including the
// family which is ignored.
const COMMAND_LOCAL: u8 = 0x0;

// \x1 : PROXY : the connection was established on behalf of another node,
// and reflects the original connection endpoints. The receiver must then use
// the information provided in the protocol block to get original the address.
const COMMAND_PROXY: u8 = 0x1;

// version and command
const VERSION_COMMAND: u8 = 0x21;

// Protocol byte

// \x00 : UNSPEC : the connection is forwarded for an unknown, unspecified
// or unsupported protocol. The sender should use this family when sending
// LOCAL commands or when dealing with unsupported protocol families. When
// used with a LOCAL command, the receiver must accept the connection and
// ignore any address information. For other commands, the receiver is free
// to accept the connection anyway and use the real endpoints addresses or to
// reject the connection. The receiver should ignore address information.
pub(super) const PROTOCOL_UNSPEC: u8 = 0x00;

// \x11 : TCP over IPv4 : the forwarded connection uses TCP over the AF_INET
// protocol family. Address length is 2*4 + 2*2 = 12 bytes.
pub(super) const PROTOCOL_TCP_IP4: u8 = 0x11;

// \x12 : UDP over IPv4 : the forwarded connection uses UDP over the AF_INET
// protocol family. Address length is 2*4 + 2*2 = 12 bytes.
pub(super) const PROTOCOL_UDP_IP4: u8 = 0x12;

//  \x21 : TCP over IPv6 : the forwarded connection uses TCP over the AF_INET6
// protocol family. Address length is 2*16 + 2*2 = 36 bytes.
pub(super) const PROTOCOL_TCP_IP6: u8 = 0x21;

// - \x22 : UDP over IPv6 : the forwarded connection uses UDP over the AF_INET6
// protocol family. Address length is 2*16 + 2*2 = 36 bytes.
pub(super) const PROTOCOL_UDP_IP6: u8 = 0x22;

// - \x31 : UNIX stream : the forwarded connection uses SOCK_STREAM over the
// AF_UNIX protocol family. Address length is 2*108 = 216 bytes.
pub(super) const PROTOCOL_UNIX_SOCKET: u8 = 0x31;

// - \x32 : UNIX datagram : the forwarded connection uses SOCK_DGRAM over the
// AF_UNIX protocol family. Address length is 2*108 = 216 bytes.
pub(super) const PROTOCOL_UNIX_DATAGRAM: u8 = 0x32;

const VALID_PROTOCOLS: &[u8] = &[
    PROTOCOL_UNSPEC,
    PROTOCOL_TCP_IP4,
    PROTOCOL_TCP_IP6,
    PROTOCOL_UDP_IP4,
    PROTOCOL_UDP_IP6,
    PROTOCOL_UNIX_SOCKET,
    PROTOCOL_UNIX_DATAGRAM,
];

// Length

pub(super) const SIZE_HEADER: u16 = 16;
const SIZE_ADDRESSES_IP4: u16 = 12;
const SIZE_ADDRESSES_IP6: u16 = 36;
const SIZE_ADDRESSES_UNIX: u16 = 216;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid header: {0}")]
    Header(String),
    #[error("Invalid IP4 address: {0}")]
    AddressIp4(String),
    #[error("Invalid IP6 address: {0}")]
    AddressIp6(String),
}

type Result<T> = std::result::Result<T, Error>;
pub(super) trait Serialize: Sized {
    fn deserialize(buf: &mut BytesMut) -> Result<Self>;
    fn serialize(&self, buf: &mut BytesMut);
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Header {
    pub(super) version_and_command: u8,
    pub(super) protocol: u8,
    pub(super) len: u16,
}

impl Header {
    pub(super) fn new(typ: SocketType) -> Self {
        let (protocol, len) = match typ {
            SocketType::Unknown => (PROTOCOL_UNSPEC, 0),
            SocketType::Ipv4 => (PROTOCOL_TCP_IP4, SIZE_ADDRESSES_IP4),
            SocketType::Ipv6 => (PROTOCOL_TCP_IP6, SIZE_ADDRESSES_IP6),
        };
        Header {
            version_and_command: VERSION_COMMAND,
            protocol,
            len,
        }
    }
}

impl Serialize for Header {
    fn deserialize(buf: &mut BytesMut) -> Result<Self> {
        if buf.len() < SIZE_HEADER as usize {
            return Err(Error::Header("Too few bytes".into()));
        }

        if &buf[0..SIGNATURE.len()] != SIGNATURE {
            return Err(Error::Header("Invalid signature".into()));
        };
        buf.advance(SIGNATURE.len());
        let version_and_command = buf.get_u8();
        if (version_and_command & 0xF0) >> 4 != PROXY_VERSION {
            return Err(Error::Header("Invalid Version".into()));
        }
        if version_and_command & 0x0F > COMMAND_PROXY {
            return Err(Error::Header("Invalid command".into()));
        }

        let protocol = buf.get_u8();
        if !VALID_PROTOCOLS.contains(&protocol) {
            return Err(Error::Header("Invalid network protocol specified".into()));
        }
        let len = buf.get_u16();
        Ok(Header {
            version_and_command,
            protocol,
            len,
        })
    }
    fn serialize(&self, buf: &mut BytesMut) {
        buf.reserve((SIZE_HEADER + self.len) as usize);
        buf.put(SIGNATURE);
        buf.put_u8(self.version_and_command);
        buf.put_u8(self.protocol);
        buf.put_u16(self.len);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Ip4Addresses {
    src_addr: u32,
    dst_addr: u32,
    src_port: u16,
    dst_port: u16,
}

impl From<(SocketAddrV4, SocketAddrV4)> for Ip4Addresses {
    fn from(addresses: (SocketAddrV4, SocketAddrV4)) -> Self {
        let (src, dst) = addresses;
        Ip4Addresses {
            src_addr: u32::from_be_bytes(src.ip().octets()),
            dst_addr: u32::from_be_bytes(dst.ip().octets()),
            src_port: src.port(),
            dst_port: dst.port(),
        }
    }
}

impl From<Ip4Addresses> for (SocketAddrV4, SocketAddrV4) {
    fn from(addresses: Ip4Addresses) -> Self {
        let src_addr = SocketAddrV4::new(
            u32::to_be_bytes(addresses.src_addr).into(),
            addresses.src_port,
        );
        let dst_addr = SocketAddrV4::new(
            u32::to_be_bytes(addresses.dst_addr).into(),
            addresses.dst_port,
        );
        (src_addr, dst_addr)
    }
}

impl Serialize for Ip4Addresses {
    fn serialize(&self, buf: &mut BytesMut) {
        buf.reserve(SIZE_ADDRESSES_IP4 as usize);
        buf.put_u32(self.src_addr);
        buf.put_u32(self.dst_addr);
        buf.put_u16(self.src_port);
        buf.put_u16(self.dst_port)
    }

    fn deserialize(buf: &mut BytesMut) -> Result<Self> {
        if buf.len() < SIZE_ADDRESSES_IP4 as usize {
            return Err(Error::AddressIp4(
                "Too short for IP4 addresses block".into(),
            ));
        }
        Ok(Ip4Addresses {
            src_addr: buf.get_u32(),
            dst_addr: buf.get_u32(),
            src_port: buf.get_u16(),
            dst_port: buf.get_u16(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Ip6Addresses {
    src_addr: [u8; 16],
    dst_addr: [u8; 16],
    src_port: u16,
    dst_port: u16,
}

impl From<(SocketAddrV6, SocketAddrV6)> for Ip6Addresses {
    fn from(addresses: (SocketAddrV6, SocketAddrV6)) -> Self {
        let (src_addr, dst_addr) = addresses;
        Ip6Addresses {
            src_addr: src_addr.ip().octets(),
            dst_addr: dst_addr.ip().octets(),
            src_port: src_addr.port(),
            dst_port: dst_addr.port(),
        }
    }
}

impl From<Ip6Addresses> for (SocketAddrV6, SocketAddrV6) {
    fn from(addresses: Ip6Addresses) -> Self {
        let src_addr = SocketAddrV6::new(addresses.src_addr.into(), addresses.src_port, 0, 0);
        let dst_addr = SocketAddrV6::new(addresses.dst_addr.into(), addresses.dst_port, 0, 0);
        (src_addr, dst_addr)
    }
}

impl Serialize for Ip6Addresses {
    fn serialize(&self, buf: &mut BytesMut) {
        buf.put(&self.src_addr[..]);
        buf.put(&self.dst_addr[..]);
        buf.put_u16(self.src_port);
        buf.put_u16(self.dst_port);
    }

    fn deserialize(buf: &mut BytesMut) -> Result<Self> {
        if buf.len() < SIZE_ADDRESSES_IP6 as usize {
            return Err(Error::AddressIp6(
                "Too short for IP6 addresses block".into(),
            ));
        }
        let mut src_addr = [0; 16];
        let mut dst_addr = [0; 16];
        (&mut src_addr[..]).copy_from_slice(&buf[0..16]);
        buf.advance(16);
        (&mut dst_addr[..]).copy_from_slice(&buf[0..16]);
        buf.advance(16);
        Ok(Ip6Addresses {
            src_addr,
            dst_addr,
            src_port: buf.get_u16(),
            dst_port: buf.get_u16(),
        })
    }
}

struct UnixAddresses {
    src_addr: [u8; 108],
    dst_addr: [u8; 108],
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_header_serialize_deserialize() {
        let h1 = Header::new(SocketType::Ipv4);
        let mut buf = BytesMut::new();
        h1.serialize(&mut buf);
        let h2 = Header::deserialize(&mut buf).expect("BUG: cannot deserialize header");
        assert_eq!(h1, h2);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_ip4_addresses_serialize_deserialize() {
        let src_addr: SocketAddrV4 = "127.0.0.1:1234".parse().expect("BUG: Cannot parse src IP");
        let dst_addr: SocketAddrV4 = "127.0.0.1:5678".parse().expect("BUG: Cannot parse dst IP");
        let a1: Ip4Addresses = (src_addr.clone(), dst_addr.clone()).into();
        let mut buf = BytesMut::new();
        a1.serialize(&mut buf);
        let a2 =
            Ip4Addresses::deserialize(&mut buf).expect("BUG: Cannot deserialize IPv4 addresses");
        assert_eq!(a1, a2);
        assert!(buf.is_empty());

        let (src_addr2, dst_addr2) = a2.into();
        assert_eq!((src_addr, dst_addr), (src_addr2, dst_addr2));
    }

    #[test]
    fn test_ip6_addresses_serialize_deserialize() {
        let src_addr: SocketAddrV6 = "[ffff:ffff:ffff:ffff:ffff:ffff:ffff:ff11]:65535"
            .parse()
            .expect("BUG: Cannot parse src IPv6");
        let dst_addr: SocketAddrV6 = "[aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aaaa:aa11]:65534"
            .parse()
            .expect("BUG: Cannot parse dst IPv6");
        let a1: Ip6Addresses = (src_addr.clone(), dst_addr.clone()).into();
        let mut buf = BytesMut::new();
        a1.serialize(&mut buf);
        let a2 =
            Ip6Addresses::deserialize(&mut buf).expect("BUG: Cannot deserialize IPv6 addresses");
        assert_eq!(a1, a2);
        assert!(buf.is_empty());

        let (src_addr2, dst_addr2) = a2.into();
        assert_eq!((src_addr, dst_addr), (src_addr2, dst_addr2));
    }
}
