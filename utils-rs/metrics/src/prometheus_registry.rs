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

pub use prometheus::{
    exponential_buckets, histogram_opts, linear_buckets, opts, Encoder, Histogram, HistogramTimer,
    HistogramVec, IntCounter, IntCounterVec, TextEncoder, DEFAULT_BUCKETS,
};

use prometheus::core::{Atomic, GenericCounter, GenericCounterVec, GenericGauge, GenericGaugeVec};
use prometheus::Registry;

/// Operates with Arc<prometheus::Registry>.
#[derive(Clone)]
pub struct MetricsRegistry {
    registry: Arc<Registry>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        let toolchain_version =
            rustc_version::version().map_or_else(|_| String::from("unknown"), |t| t.to_string());
        Self::new(&[
            (
                &"semantic",
                &std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "Unknown".to_string()),
            ),
            (&"revision", &ii_scm::version_git!()),
            (&"toolchain", &toolchain_version),
        ])
    }
}

/// Provides registry for counting metrics and histograms.
///
/// Buckets is vector of floats as it is produced by prometheus functions
/// [`prometheus::exponential_buckets`] or [`prometheus::linear_buckets`] that are reexported
/// for convenience. Alternatively reexported constant [`prometheus::DEFAULT_BUCKETS`] can be used.
impl MetricsRegistry {
    /// Creates new metrics registry and provides associated metadata to the application.
    /// The information is slice of ('label', 'value') tuples: e. g.:
    /// `[("rust", "1.47"), ("version", "1.5.4")]`
    /// This metadata can be extracted or joined with prometheus query
    /// ```text
    ///   some_metric{job="static"}
    /// * on (instance, job) group_left(version)
    ///   application_version_details{job="static"}
    /// ```
    pub fn new(application_version_details: &[(&str, &str)]) -> Self {
        let registry: Arc<Registry> = Default::default();
        let version_details_gauge = GenericGaugeVec::<prometheus::core::AtomicU64>::new(
            opts!(
                "application_version_details",
                "Version details of the application producing time series"
            ),
            &application_version_details
                .iter()
                .map(|(label, _)| *label)
                .collect::<Vec<_>>(),
        )
        .expect("BUG: Couldn't set up app version details");

        registry
            .register(Box::new(version_details_gauge.clone()))
            .expect("BUG: Failed to register version details");
        version_details_gauge
            .with_label_values(
                &application_version_details
                    .iter()
                    .map(|(_, label_values)| *label_values)
                    .collect::<Vec<_>>(),
            )
            .set(1);

        Self { registry }
    }

    pub fn register_generic_gauge<T: Atomic + 'static>(
        &self,
        name: &str,
        help: &str,
    ) -> GenericGauge<T> {
        let gauge = GenericGauge::with_opts(opts!(name, help))
            .expect("BUG: Couldn't build generic_gauge with opts");
        self.registry
            .register(Box::new(gauge.clone()))
            .expect("BUG: Couldn't register generic_gauge");
        gauge
    }

    pub fn register_generic_gauge_vec<T: Atomic + 'static>(
        &self,
        name: &str,
        help: &str,
        label_names: &[&str],
    ) -> GenericGaugeVec<T> {
        let gauge_vec = GenericGaugeVec::new(opts!(name, help), label_names)
            .expect("BUG: Couldn't build generic_gauge with opts");
        self.registry
            .register(Box::new(gauge_vec.clone()))
            .expect("BUG: Couldn't register generic_gauge");
        gauge_vec
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
        let counter_vec = GenericCounterVec::new(opts!(name, help), label_names)
            .expect("BUG: Couldn't build generic_counter_vec with opts");
        self.registry
            .register(Box::new(counter_vec.clone()))
            .expect("BUG: Couldn't register generic_counter_vec");
        counter_vec
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
