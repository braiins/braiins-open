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

use super::error::{Error, Result};
use std::convert::TryFrom;
use std::net::SocketAddr;

pub mod v1;
pub mod v2;

pub(crate) const MAX_HEADER_SIZE: usize = 536;

/// Type of transport
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum SocketType {
    /// TCP/IP V4
    Ipv4,
    /// TCP/IP V6
    Ipv6,
    /// Transport protocol in unknown
    Unknown,
}

/// Contains information from PROXY protocol
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ProxyInfo {
    /// Type of transport
    pub socket_type: SocketType,
    /// Original source address passed in PROXY protocol
    pub original_source: Option<SocketAddr>,
    /// Original destination address passed in PROXY protocol
    pub original_destination: Option<SocketAddr>,
}

impl TryFrom<(Option<SocketAddr>, Option<SocketAddr>)> for ProxyInfo {
    type Error = Error;
    fn try_from(addrs: (Option<SocketAddr>, Option<SocketAddr>)) -> Result<Self> {
        match (addrs.0, addrs.1) {
            (s @ Some(SocketAddr::V4(_)), d @ Some(SocketAddr::V4(_))) => Ok(ProxyInfo {
                socket_type: SocketType::Ipv4,
                original_source: s,
                original_destination: d,
            }),

            (s @ Some(SocketAddr::V6(_)), d @ Some(SocketAddr::V6(_))) => Ok(ProxyInfo {
                socket_type: SocketType::Ipv6,
                original_source: s,
                original_destination: d,
            }),

            (None, None) => Ok(ProxyInfo {
                socket_type: SocketType::Unknown,
                original_source: None,
                original_destination: None,
            }),

            _ => Err(Error::Proxy(
                "Inconsistent source and destination addresses".into(),
            )),
        }
    }
}
