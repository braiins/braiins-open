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
    exponential_buckets, histogram_opts, linear_buckets, opts, Encoder, Histogram, HistogramTimer,
    HistogramVec, IntCounter, IntCounterVec, TextEncoder, DEFAULT_BUCKETS,
};

use prometheus::core::{Atomic, GenericCounter, GenericCounterVec, GenericGauge, GenericGaugeVec};
use prometheus::Registry;

/// Operates with Arc<prometheus::Registry>.
#[derive(Default, Clone)]
pub struct MetricsRegistry {
    registry: std::sync::Arc<Registry>,
}

/// Provides registry for counting metrics and histograms.
///
/// Buckets is vector of floats as it is produced by prometheus functions
/// [`prometheus::exponential_buckets`] or [`prometheus::linear_buckets`] that are reexported
/// for convenience. Alternatively reexported constant [`prometheus::DEFAULT_BUCKETS`] can be used.
impl MetricsRegistry {
    pub fn register_generic_gauge<T: Atomic + 'static>(
        &self,
        name: &str,
        help: &str,
    ) -> GenericGauge<T> {
        let counter = GenericGauge::with_opts(opts!(name, help))
            .expect("BUG: Couldn't build generic_gauge with opts");
        self.registry
            .register(Box::new(counter.clone()))
            .expect("BUG: Couldn't register generic_gauge");
        counter
    }

    pub fn register_generic_gauge_vec<T: Atomic + 'static>(
        &self,
        name: &str,
        help: &str,
        label_names: &[&str],
    ) -> GenericGaugeVec<T> {
        let counter = GenericGaugeVec::new(opts!(name, help), label_names)
            .expect("BUG: Couldn't build generic_gauge with opts");
        self.registry
            .register(Box::new(counter.clone()))
            .expect("BUG: Couldn't register generic_gauge");
        counter
    }

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

    pub fn register_histogram(&self, name: &str, help: &str, buckets: Vec<f64>) -> Histogram {
        let histogram = Histogram::with_opts(histogram_opts!(name, help, buckets))
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
        buckets: Vec<f64>,
    ) -> HistogramVec {
        let histogram = HistogramVec::new(histogram_opts!(name, help, buckets), label_names)
            .expect("BUG: Couldn't build histogram_vec with opts");
        self.registry
            .register(Box::new(histogram.clone()))
            .expect("BUG: Couldn't register histogram_vec");
        histogram
    }
    pub fn to_text(&self) -> crate::Result<(Vec<u8>, String)> {
        let mut buffer = vec![];
        let mut metric_families = self.registry.gather();
        // Collect also default metric families results in appending 'process' related metrics that
        // are provided by the default registry.
        let mut default_metric_families = prometheus::gather();
        metric_families.append(&mut default_metric_families);
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok((buffer, String::from(encoder.format_type())))
    }
}
