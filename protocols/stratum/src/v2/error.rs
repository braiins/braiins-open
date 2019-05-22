//! Version 2 errors only

use failure::Fail;

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "Unknown message error: {}", _0)]
    UnknownMessage(String),
}
