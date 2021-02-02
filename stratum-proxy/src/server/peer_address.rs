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

use std::{fmt, net::SocketAddr};

/// Downstream peer representation as a direct peer address with optional original peer address
/// known, for example from PROXY protocol.
#[derive(Copy, Clone, Debug)]
pub struct DownstreamPeer {
    direct_peer: SocketAddr,
    original_peer: Option<SocketAddr>,
}

impl DownstreamPeer {
    pub fn new(direct_peer: SocketAddr) -> Self {
        Self {
            direct_peer,
            original_peer: None,
        }
    }

    pub fn add_original_peer(&mut self, original_peer: SocketAddr) {
        self.original_peer.replace(original_peer);
    }

    pub fn direct_peer(&self) -> SocketAddr {
        self.direct_peer
    }
}

impl fmt::Display for DownstreamPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}(original_peer:{})",
            self.direct_peer.to_string(),
            self.original_peer
                .map_or_else(|| "N/A".to_string(), |s| s.to_string())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::DownstreamPeer;
    use std::net::{IpAddr, SocketAddr};

    #[test]
    fn correct_downstream_peer_format() {
        let mut peer = DownstreamPeer::new(SocketAddr::new(IpAddr::from([5, 4, 3, 2]), 5432));
        assert_eq!(
            format!("{}", peer),
            String::from("5.4.3.2:5432(original_peer:N/A)")
        );
        peer.add_original_peer(SocketAddr::new(IpAddr::from([4, 5, 6, 7]), 4567));
        assert_eq!(
            format!("{}", peer),
            String::from("5.4.3.2:5432(original_peer:4.5.6.7:4567)")
        );
    }
}
