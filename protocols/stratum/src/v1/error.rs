//! Version 1 errors only

use failure::{Backtrace, Context, Fail};
use std::fmt::{self, Display};

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ErrorKind {
    /// Json error.
    #[fail(display = "JSON error: {}", _0)]
    Json(String),

    #[fail(display = "Rpc error: {}", _0)]
    Rpc(String),

    #[fail(display = "Subscription error: {}", _0)]
    Subscribe(String),
}
