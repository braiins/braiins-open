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

use std::sync::Arc;

#[cfg_attr(not(feature = "prometheus_metrics"), path = "dummy.rs")]
mod prometheus_registry;

/// Reexport primitives necessary for implementing Collector
pub use prometheus_registry::*;

/// Generic interface for Collector instantiation
pub trait MetricsCollectorBuilder {
    type Collector;
    fn build_metrics_collector(&self) -> Arc<Self::Collector>;
    fn to_text(&self) -> Result<(Vec<u8>, String)>;
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[cfg(feature = "prometheus_metrics")]
    #[error("Prometheus metric processing related error: {0}")]
    PrometheusError(#[from] prometheus::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
