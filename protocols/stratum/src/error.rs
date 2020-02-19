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

//! Module that represents stratum protocol errors

use failure::{Backtrace, Context, Fail};
use std;
use std::fmt::{self, Display};
use std::io;

use ii_async_compat::tokio_util;

#[derive(Debug)]
pub struct Error {
    inner: Context<ErrorKind>,
}

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ErrorKind {
    /// Input/Output error.
    #[fail(display = "I/O error: {}", _0)]
    Io(String),

    /// Errors emitted by serde
    #[fail(display = "Serde: {}", _0)]
    Serde(String),

    /// General error used for more specific .
    #[fail(display = "General error: {}", _0)]
    General(String),

    /// Unexpected version of something.
    #[fail(display = "Unexpected {} version: {}, expected: {}", _0, _1, _2)]
    UnexpectedVersion(String, String, String),

    #[fail(display = "Noise handshake error: {}", _0)]
    Noise(String),

    /// Stratum version 1 error
    #[fail(display = "V1 error: {}", _0)]
    V1(super::v1::error::ErrorKind),
    /// Stratum version 2 error
    #[fail(display = "V2 error: {}", _0)]
    V2(super::v2::error::ErrorKind),
}

/// Implement Fail trait instead of use Derive to get more control over custom type.
/// The main advantage is customization of Context type which allows conversion of
/// any error types to this custom error with general error kind by calling context
/// method on any result type.
impl Fail for Error {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.inner.get_context().clone()
    }

    pub fn into_inner(self) -> Context<ErrorKind> {
        self.inner
    }
}

/// Convenience conversion to Error from ErrorKind that carries the context
impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            inner: Context::new(kind),
        }
    }
}

/// V1 Protocol version specific convenience conversion to Error
impl From<super::v1::error::ErrorKind> for Error {
    fn from(kind: super::v1::error::ErrorKind) -> Self {
        ErrorKind::V1(kind).into()
    }
}

/// V2 Protocol version specific convenience conversion to Error
impl From<super::v2::error::ErrorKind> for Error {
    fn from(kind: super::v2::error::ErrorKind) -> Self {
        ErrorKind::V2(kind).into()
    }
}

impl From<Context<ErrorKind>> for Error {
    fn from(inner: Context<ErrorKind>) -> Self {
        Self { inner }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::Io(msg)),
        }
    }
}

impl From<fmt::Error> for Error {
    fn from(e: fmt::Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::General(msg)),
        }
    }
}

impl From<tokio_util::codec::LinesCodecError> for Error {
    fn from(e: tokio_util::codec::LinesCodecError) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::Io(msg)),
        }
    }
}

impl From<snow::error::Error> for Error {
    fn from(e: snow::error::Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::Noise(msg)),
        }
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::General(msg)),
        }
    }
}

impl From<Context<&str>> for Error {
    fn from(context: Context<&str>) -> Self {
        Self {
            inner: context.map(|info| ErrorKind::General(info.to_string())),
        }
    }
}

impl From<Context<String>> for Error {
    fn from(context: Context<String>) -> Self {
        Self {
            inner: context.map(|info| ErrorKind::General(info)),
        }
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(e: serde_json::error::Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::Serde(msg)),
        }
    }
}

impl From<super::v2::serialization::Error> for Error {
    fn from(e: super::v2::serialization::Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::Serde(msg)),
        }
    }
}

/// A specialized `Result` type bound to [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Re-export failure's ResultExt for easier usage
pub use failure::ResultExt;
