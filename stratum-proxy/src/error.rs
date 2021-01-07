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

//! Module that represents custom stratum proxy errors

use std::error::Error as StdError;
use std::io;
use thiserror::Error;

use ii_wire::proxy::error::Error as ProxyError;

#[derive(Error, Debug)]
pub enum DownstreamError {
    #[error("Early network error before any protocol communication started")]
    EarlyIo(std::io::Error),
    #[error("PROXY protocol error: {0}")]
    ProxyProtocol(ProxyError),
    #[error("Stratum error: {0}")]
    Stratum(ii_stratum::error::Error),
    #[error("Timeout error: {0}")]
    Timeout(tokio::time::error::Elapsed),
}

#[derive(Error, Debug)]
pub enum UpstreamError {
    #[error("IO error: {0}")]
    Io(std::io::Error),
    #[error("PROXY protocol error: {0}")]
    ProxyProtocol(ProxyError),
    #[error("Stratum error: {0}")]
    Stratum(ii_stratum::error::Error),
    #[error("Timeout error: {0}")]
    Timeout(tokio::time::error::Elapsed),
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

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to resolved host: {0}")]
    HostNameError(String),

    /// General error used for more specific errors.
    #[error("General error: {0}")]
    General(String),

    #[error("General error: {0} with metrics label: {1}")]
    GeneralWithMetricsLabel(String, &'static str),

    /// Stratum protocol error.
    #[error("Stratum error: {0}")]
    Stratum(#[from] ii_stratum::error::Error),

    /// Bitcoin Hashes error.
    #[error("Bitcoin Hashes error: {0}")]
    BitcoinHashes(#[from] bitcoin_hashes::error::Error),

    /// Timeout error.
    #[error("Timeout error: {0}")]
    Timeout(/*#[from]*/ tokio::time::error::Elapsed),

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

    /// Prometheus metrics error.
    #[cfg(feature = "prometheus_metrics")]
    #[error("Metrics error: {0}")]
    Metrics(#[from] ii_metrics::Error),

    /// Errors when communicatin with downstream node
    #[error("Downstream error: {0}")]
    Downstream(#[from] DownstreamError),

    /// Errors when communicating with upstream node
    #[error("Downstream error: {0}")]
    Upstream(#[from] UpstreamError),

    /// Stratum protocol state error
    #[error("Stratum V2 protocol state related error: {0}")]
    Protocol(V2ProtocolError),

    #[error("I/O error: {0}")]
    Io(std::io::Error),
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
