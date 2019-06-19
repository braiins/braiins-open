//! Stratum proxy library provides functionality for proxying any combination of Stratum V1 and V2
//! protocol version

#![feature(await_macro, async_await)]

pub mod error;
pub mod server;
pub mod translation;
