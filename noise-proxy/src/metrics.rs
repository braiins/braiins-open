// Copyright (C) 2021  Braiins Systems s.r.o.
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

use ii_metrics::MetricsRegistry;
use prometheus::IntCounterVec;

pub struct NoiseProxyMetrics {
    tcp_connection_open_total: IntCounterVec,
    tcp_connection_close_stage: IntCounterVec,
}

impl NoiseProxyMetrics {
    pub fn new() -> (Arc<Self>, MetricsRegistry) {
        let registry = MetricsRegistry::default();
        let collector = Self::from_registry(&registry);
        (collector, registry)
    }

    pub fn from_registry(registry: &MetricsRegistry) -> Arc<Self> {
        Arc::new(NoiseProxyMetrics {
            tcp_connection_open_total: registry.register_generic_counter_vec(
                "noise_tcp_connection_open",
                "Number of TCP-open events",
                &["result"],
            ),
            tcp_connection_close_stage: registry.register_generic_counter_vec(
                "noise_tcp_connection_close",
                "Number of TCP-close events",
                &["result"],
            ),
        })
    }
}

impl NoiseProxyMetrics {
    pub fn account_successful_tcp_open(&self) {
        self.tcp_connection_open_total
            .with_label_values(&["success"])
            .inc();
    }

    pub fn account_failed_tcp_open(&self) {
        self.tcp_connection_open_total
            .with_label_values(&["fail"])
            .inc();
    }

    pub fn account_normal_tcp_close(&self) {
        self.tcp_connection_close_stage
            .with_label_values(&["ok"])
            .inc();
    }

    pub fn account_tcp_close_due_error(&self) {
        self.tcp_connection_close_stage
            .with_label_values(&["error"])
            .inc();
    }
}
