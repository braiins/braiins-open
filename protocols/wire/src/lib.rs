#![feature(async_await, await_macro)]

// Tokio is re-exported here for the benefit of dependant crates.
// That way, the Tokio dependency is specified in one place (in wire/Cargo.toml).
pub use tokio;

mod messaging;
pub use messaging::*;

mod network;
pub use network::*;

mod framing;
pub use framing::*;

pub mod utils;
