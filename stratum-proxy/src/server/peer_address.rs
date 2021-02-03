// Copyright (C) 2021  Braiins Systems s.r.o.
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

//! Module contains primitives for deeper peer information tracking

use ii_wire::proxy::ProxyInfo;
use std::{fmt, net::SocketAddr};

/// Downstream peer representation as a direct peer address with optional original peer address
/// known, for example from PROXY protocol.
#[derive(Copy, Clone, Debug)]
pub struct DownstreamPeer {
    pub direct_peer: SocketAddr,
    /// Track additional information about the peer
    pub proxy_info: Option<ii_wire::proxy::ProxyInfo>,
}

impl DownstreamPeer {
    pub fn new(direct_peer: SocketAddr) -> Self {
        Self {
            direct_peer,
            proxy_info: None,
        }
    }

    pub fn set_proxy_info(&mut self, proxy_info: ProxyInfo) {
        self.proxy_info.replace(proxy_info);
    }
}

impl fmt::Display for DownstreamPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}({})",
            self.direct_peer.to_string(),
            self.proxy_info
                .map(|s| s.to_string())
                .unwrap_or_else(|| "ProxyInfo[N/A]".to_string())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::DownstreamPeer;
    use ii_wire::proxy::ProxyInfo;
    use std::convert::TryFrom;
    use std::net::{IpAddr, SocketAddr};

    #[test]
    fn correct_downstream_peer_format() {
        let src = SocketAddr::new(IpAddr::from([4, 5, 6, 7]), 4567);
        let dst = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let proxy_info =
            ProxyInfo::try_from((Some(src), Some(dst))).expect("BUG: cannot produce proxy info");

        let mut peer = DownstreamPeer::new(SocketAddr::new(IpAddr::from([5, 4, 3, 2]), 5432));
        assert_eq!(
            format!("{}", peer),
            String::from("5.4.3.2:5432(ProxyInfo[N/A])")
        );
        peer.set_proxy_info(proxy_info);
        assert_eq!(
            format!("{}", peer),
            String::from("5.4.3.2:5432(ProxyInfo[SRC:4.5.6.7:4567, DST:1.2.3.4:1234])")
        );
    }
}
