//! Module that represents custom stratum proxy errors

use failure::{Backtrace, Context, Fail};
use std;
use std::fmt::{self, Display};
use std::io;

#[derive(Debug)]
pub struct Error {
    inner: Context<ErrorKind>,
}

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ErrorKind {
    /// General error used for more specific .
    #[fail(display = "General error: {}", _0)]
    General(String),

    /// General error used for more specific .
    #[fail(display = "Stratum error: {}", _0)]
    Stratum(ii_stratum::error::ErrorKind),

    /// Bitcoin Hashes error.
    #[fail(display = "Bitcoin Hashes error: {}", _0)]
    BitcoinHashes(String),

    /// Input/Output error.
    #[fail(display = "I/O error: {}", _0)]
    Io(String),

    /// CLI usage / configuration error
    #[fail(display = "Could not parse `{}` as IP address", _0)]
    BadIp(String),
}

/// Implement Fail trait instead of use Derive to get more control over custom type.
/// The main advantage is customization of Context type which allows conversion of
/// any error types to this custom error with general error kind by calling context
/// method on any result type.
impl Fail for Error {
    fn cause(&self) -> Option<&Fail> {
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

impl From<ii_stratum::error::Error> for Error {
    fn from(e: ii_stratum::error::Error) -> Self {
        Self {
            inner: e.into_inner().map(|kind| ErrorKind::Stratum(kind)),
        }
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

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::General(msg)),
        }
    }
}

impl From<bitcoin_hashes::error::Error> for Error {
    fn from(e: bitcoin_hashes::error::Error) -> Self {
        let msg = e.to_string();
        Self {
            inner: e.context(ErrorKind::BitcoinHashes(msg)),
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

impl From<Context<ErrorKind>> for Error {
    fn from(context: Context<ErrorKind>) -> Self {
        Self { inner: context }
    }
}

/// A specialized `Result` type bound to [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Re-export failure's ResultExt for easier usage
pub use failure::ResultExt;
