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

pub use prometheus::{
    histogram_opts, opts, Encoder, Histogram, HistogramTimer, HistogramVec, IntCounter,
    IntCounterVec, TextEncoder,
};

use prometheus::core::{Atomic, GenericCounter, GenericCounterVec};
use prometheus::Registry;

/// Operates with Arc<prometheus::Registry>.
#[derive(Default, Clone)]
pub struct PrometheusRegistry {
    registry: std::sync::Arc<Registry>,
}

impl PrometheusRegistry {
    pub fn register_generic_counter<T: Atomic + 'static>(
        &self,
        name: &str,
        help: &str,
    ) -> GenericCounter<T> {
        let counter = GenericCounter::with_opts(opts!(name, help))
            .expect("BUG: Couldn't build generic_counter with opts");
        self.registry
            .register(Box::new(counter.clone()))
            .expect("BUG: Couldn't register generic_counter");
        counter
    }

    pub fn register_generic_counter_vec<T: Atomic + 'static>(
        &self,
        name: &str,
        help: &str,
        label_names: &[&str],
    ) -> GenericCounterVec<T> {
        let counter = GenericCounterVec::new(opts!(name, help), label_names)
            .expect("BUG: Couldn't build generic_counter_vec with opts");
        self.registry
            .register(Box::new(counter.clone()))
            .expect("BUG: Couldn't register generic_counter_vec");
        counter
    }

    pub fn register_histogram(&self, name: &str, help: &str) -> Histogram {
        let histogram = Histogram::with_opts(histogram_opts!(name, help))
            .expect("BUG: Couldn't build histogram with opts");
        self.registry
            .register(Box::new(histogram.clone()))
            .expect("BUG: Couldn't register histogram");
        histogram
    }

    pub fn register_histogram_vec(
        &self,
        name: &str,
        help: &str,
        label_names: &[&str],
    ) -> HistogramVec {
        let histogram = HistogramVec::new(histogram_opts!(name, help), label_names)
            .expect("BUG: Couldn't build histogram_vec with opts");
        self.registry
            .register(Box::new(histogram.clone()))
            .expect("BUG: Couldn't register histogram_vec");
        histogram
    }
    pub fn to_text(&self) -> crate::Result<(Vec<u8>, String)> {
        let mut buffer = vec![];
        let metric_families = self.registry.gather();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok((buffer, String::from(encoder.format_type())))
    }
}
