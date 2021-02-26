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

use crate::tokio;

use thiserror::Error;

/// Error type for this module
#[derive(Error, Debug)]
pub enum Error {
    #[error("Proxy protocol error: {0}")]
    Proxy(String),

    #[error("Proxy protocol V2 error: {0}")]
    ProxyV2(#[from] crate::proxy::codec::v2::proto::Error),

    #[error("IO error: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("Invalid encoding of proxy header: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("Invalid address in proxy header: {0}")]
    IPAddress(#[from] std::net::AddrParseError),

    #[error("Invalid port in proxy header: {0}")]
    Port(#[from] std::num::ParseIntError),

    #[error("Invalid state: {0}")]
    InvalidState(String),
}

/// Convenient Result type, with our Error included
pub type Result<T> = std::result::Result<T, Error>;
