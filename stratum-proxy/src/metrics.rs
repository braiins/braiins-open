use crate::error::Result;
use crate::translation::V2ToV1Translation;
use ii_logging::macros::*;
use once_cell::sync;
use primitive_types::U256;
use prometheus::{labels, opts, register_int_counter_vec, Encoder, IntCounterVec, TextEncoder};
use std::convert::TryInto;
use std::sync::Arc;
use tokio::time::Duration;

/// Combines all metrics and provides additional tooling for accounting shares/submits
///
/// All metrics have the following constant labels:
/// region - which area the software operates in
/// hostname - name of the machine where the metrics are being taken
#[derive(Debug)]
pub struct Metrics {
    /// TCP connections, labels:
    /// - type = (open,close)
    tcp_connection_event_total: IntCounterVec,
    /// Aggregate of submitted shares, labels:
    /// - type = (downstream, upstream)
    /// - status = (accepted, rejected)
    shares_total: IntCounterVec,
    /// Aggregate of submits => 1 submit = DIFF shares, labels:
    /// - type = (downstream, upstream)
    /// - status = (accepted, rejected)
    submits_total: IntCounterVec,
}

impl Metrics {
    pub fn new() -> Self {
        // TODO figure out how to get additional information from configuration
        let hostname = gethostname::gethostname()
            .into_string()
            .expect("BUG: cannot fetch hostname");
        let const_labels = labels! {
           "region" => "eu",
           "host" => hostname.as_str(),
        };

        let variable_label_names = &["status"];

        Self {
            tcp_connection_event_total: register_int_counter_vec!(
                opts!(
                    "tcp_connection_event_total",
                    "Number of total connection events",
                    const_labels.clone()
                ),
                &["type"]
            )
            .expect("BUG: cannot build tcp_connection_event_total"),

            shares_total: register_int_counter_vec!(
                opts!(
                    "shares_total",
                    "Sum of shares difficulty that have been processed",
                    const_labels.clone()
                ),
                &["direction", "status"]
            )
            .expect("BUG: cannot build shares_total"),

            submits_total: register_int_counter_vec!(
                opts!(
                    "submits_total",
                    "Sum of submits that have processed",
                    const_labels.clone()
                ),
                variable_label_names
            )
            .expect("BUG: cannot build submits_total"),
        }
    }

    /// Helper that accounts a share if `target` is provided among timeseries specified by
    /// `label_values`. If no target is specified only submit is accounted
    pub fn account_share(&self, target: Option<U256>, label_values: &[&str]) {
        if let Some(tgt) = target {
            let share_value = V2ToV1Translation::target_to_diff(tgt)
                .try_into()
                .expect("BUG: Failed to convert target difficulty");
            // TODO add region and host
            self.shares_total
                .with_label_values(label_values)
                .inc_by(share_value);
        }
        self.submits_total.with_label_values(label_values).inc();
    }

    pub fn account_accepted_share(&self, target: Option<U256>) {
        self.account_share(target, &["accepted"]);
    }

    pub fn account_rejected_share(&self, target: Option<U256>) {
        self.account_share(target, &["rejected"]);
    }

    pub fn account_opened_connection(&self) {
        self.tcp_connection_event_total
            .with_label_values(&["open"])
            .inc();
    }
    pub fn account_closed_connection(&self) {
        self.tcp_connection_event_total
            .with_label_values(&["close"])
            .inc();
    }

    /// TODO rename this to stats_log_task
    pub fn spawn_stats(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                match to_text() {
                    Ok((stats_buf, _)) => {
                        match std::str::from_utf8(&stats_buf) {
                            Ok(metrics_str) => info!("Metrics:\n {}", metrics_str),
                            Err(e) => error!("Cannot convert metrics to string: {:?}", e),
                        };
                    }
                    Err(e) => error!("Cannot dump metrics into log: {}", e),
                }
            }
        });
    }
}

/// Converts all metrics from default registry to text
pub fn to_text() -> Result<(Vec<u8>, String)> {
    let mut buffer = vec![];
    let metric_families = prometheus::gather();
    trace!("metrics: Gathered {:?}", metric_families);
    let encoder = TextEncoder::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok((buffer, String::from(encoder.format_type())))
}

/// Globally accessible metrics that are registered in the default registry
/// TODO remove this as the global instance will not be needed
pub static METRICS: sync::Lazy<Metrics> = sync::Lazy::new(|| Metrics::new());
