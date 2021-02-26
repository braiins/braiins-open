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
#![allow(clippy::single_component_path_imports)]

#[cfg(all(feature = "tokio03", feature = "tokio02"))]
compile_error!("You can't use both Tokio 0.3 and 0.2. Note: The `tokio02` feature requires default features to be turned off");

#[cfg(feature = "tokio12")]
pub(crate) use tokio;
#[cfg(feature = "tokio12")]
pub(crate) use tokio_util;

#[cfg(feature = "tokio03")]
pub(crate) use tokio03_core as tokio;
#[cfg(feature = "tokio03")]
pub(crate) use tokio03_util as tokio_util;

#[cfg(feature = "tokio02")]
pub(crate) use tokio02_core as tokio;
#[cfg(feature = "tokio02")]
pub(crate) use tokio02_util as tokio_util;

#[cfg(feature = "bytes")]
pub(crate) use bytes;
#[cfg(feature = "bytes05")]
pub(crate) use bytes05 as bytes;
#[cfg(feature = "bytes06")]
pub(crate) use bytes06 as bytes;

#[macro_use]
extern crate ii_logging;

mod connection;
pub use connection::*;

mod server;
pub use server::*;

mod client;
pub use client::*;

mod framing;
pub use framing::*;

pub mod proxy;
