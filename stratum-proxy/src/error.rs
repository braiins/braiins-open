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

//! Module that represents custom stratum proxy errors

use std::error::Error as StdError;
use std::io;
use thiserror::Error;

use crate::metrics::ErrorLabeling;
use ii_wire::proxy::error::Error as ProxyError;

/// TODO split into upstream/downstream to eliminate repetitive naming
#[derive(Error, Debug)]
pub enum GeneralNetworkError {
    #[error("Early network error before any protocol communication started")]
    DownstreamEarlyIo(std::io::Error),
    #[error("Downstream stratum error: {0}")]
    DownstreamStratum(ii_stratum::error::Error),
    #[error("Downstream timeout error: {0}")]
    DownstreamTimeout(tokio::time::error::Elapsed),
    #[error("Upstream IO error: {0}")]
    UpstreamIo(std::io::Error),
    #[error("Upstream PROXY protocol error: {0}")]
    UpstreamProxyProtocol(ProxyError),
    #[error("Upstream stratum error: {0}")]
    UpstreamStratum(ii_stratum::error::Error),
    #[error("Upstream timeout error: {0}")]
    UpstreamTimeout(tokio::time::error::Elapsed),
}

#[derive(Error, Debug)]
pub enum V2ProtocolError {
    #[error("V2 Setup Connection error: {0}")]
    SetupConnection(String),
    #[error("V2 Open Mining Channel error: {0}")]
    OpenMiningChannel(String),
    #[error("V2 Other non-specified Error: {0}")]
    Other(String),
}

impl V2ProtocolError {
    pub fn setup_connection<T: StdError>(val: T) -> Self {
        Self::SetupConnection(val.to_string())
    }
    pub fn open_mining_channel<T: StdError>(val: T) -> Self {
        Self::OpenMiningChannel(val.to_string())
    }
    pub fn other<T: StdError>(val: T) -> Self {
        Self::Other(val.to_string())
    }
}

impl ErrorLabeling for V2ProtocolError {
    fn label(&self) -> String {
        match self {
            Self::SetupConnection(_) => "setup_connection".to_string(),
            Self::OpenMiningChannel(_) => "open_mining_channel".to_string(),
            Self::Other(_) => "protocol_other".to_string(),
        }
    }
}

impl ErrorLabeling for Error {
    fn label(&self) -> String {
        use crate::error::Error::*;
        use ii_stratum::error::Error as StratumError;
        match self {
            General(_) => "other".to_string(),
            Stratum(s) => match s {
                StratumError::Noise(_) => "noise",
                StratumError::NoiseEncoding(_) => "noise",
                StratumError::NoiseProtocol(_) => "noise",
                StratumError::V2(_) => "downstream",
                StratumError::V1(_) => "upstream",
                StratumError::NoiseSignature(_) => "noise",
                _ => "stratum_other",
            }
            .to_string(),
            Protocol(p) => p.label(),
            GeneralNetwork(err) => match err {
                GeneralNetworkError::DownstreamEarlyIo(_) => "early",
                GeneralNetworkError::UpstreamIo(_)
                | GeneralNetworkError::UpstreamProxyProtocol(_)
                | GeneralNetworkError::UpstreamStratum(_)
                | GeneralNetworkError::UpstreamTimeout(_) => "upstream",
                GeneralNetworkError::DownstreamStratum(_)
                | GeneralNetworkError::DownstreamTimeout(_) => "downstream",
            }
            .to_string(),
            ProxyProtocol(_) => "haproxy".to_string(),
            Utf8(_) => "utf8".to_string(),
            Json(_) => "json".to_string(),
            Label(s, _) => s.clone(),
            _ => "other".to_string(),
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    /// General error used for more specific errors.
    #[error("General error: {0}")]
    General(String),

    /// Stratum protocol error.
    #[error("Stratum error: {0}")]
    Stratum(#[from] ii_stratum::error::Error),

    /// Bitcoin Hashes error.
    #[error("Bitcoin Hashes error: {0}")]
    BitcoinHashes(#[from] bitcoin_hashes::error::Error),

    /// Timeout error.
    #[error("Timeout error: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),

    /// Utf8 error
    #[error("Error decoding UTF-8 string: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// Json Error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Connection Attempt Error
    #[error("Client connection attempt error: {0}")]
    ClientAttempt(#[from] ii_wire::AttemptError),

    /// File content error
    #[error("Invalid content of key/certificate file: {0}")]
    InvalidFile(String),

    /// PROXY protocol error
    #[error("PROXY protocol error: {0}")]
    ProxyProtocol(#[from] ProxyError),

    /// Prometheus metrics error.
    #[error("Metrics error: {0}")]
    Metrics(#[from] prometheus::Error),

    /// General network errors
    #[error("General network error: {0}")]
    GeneralNetwork(GeneralNetworkError),

    /// Stratum protocol state error
    #[error("Stratum protocol state related error: {0}")]
    Protocol(V2ProtocolError),

    /// Generic error given by label
    #[error("Generic error: {1} with label: {0}")]
    Label(String, String),

    #[error("I/O error: {0}")]
    Io(std::io::Error),
}

impl From<GeneralNetworkError> for Error {
    fn from(val: GeneralNetworkError) -> Self {
        Self::GeneralNetwork(val)
    }
}

impl From<V2ProtocolError> for Error {
    fn from(val: V2ProtocolError) -> Self {
        Self::Protocol(val)
    }
}

impl<T> From<futures::channel::mpsc::TrySendError<T>> for Error
where
    T: Send + Sync + 'static,
{
    fn from(e: futures::channel::mpsc::TrySendError<T>) -> Self {
        Error::Io(io::Error::new(io::ErrorKind::Other, e))
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::General(msg.to_string())
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::General(msg)
    }
}

/// A specialized `Result` type bound to [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
