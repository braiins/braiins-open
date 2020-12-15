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

use crate::error;
use crate::translation::V2ToV1Translation;
use ii_logging::macros::*;
use ii_metrics::MetricsRegistry;
use ii_stratum::v1::rpc::Method;
pub use primitive_types::U256;
use std::convert::TryInto;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;

pub use prometheus::{
    histogram_opts, opts, Encoder, Histogram, HistogramTimer, HistogramVec, IntCounter,
    IntCounterVec, TextEncoder,
};

#[derive(Debug)]
pub struct TcpConnectionCloseTotal(IntCounterVec);

impl TcpConnectionCloseTotal {
    pub fn new(registry: &MetricsRegistry) -> Self {
        Self(registry.register_generic_counter_vec(
            "tcp_connection_close_stage",
            "Number of tcp connection close events in a particular stage",
            &["stage"],
        ))
    }

    pub fn inc_by_error(&self, error: &crate::error::Error) {
        let stage_label = error.label();
        self.0.with_label_values(&[stage_label]).inc();
    }
    pub fn inc_as_ok(&self) {
        self.0.with_label_values(&["ok"]);
    }
}

#[derive(Default, Clone)]
pub struct ProxyCollectorBuilder(MetricsRegistry);

impl ProxyCollectorBuilder {
    pub fn new(registry: MetricsRegistry) -> Self {
        Self(registry)
    }

    pub fn build_metrics_collector(&self) -> Arc<ProxyMetrics> {
        let variable_label_names = &["direction", "status"];

        Arc::new(ProxyMetrics {
            tcp_connection_open_total: self.0.register_generic_counter(
                "tcp_connection_open_total",
                "Number of connection open events",
            ),
            tcp_connection_close_stage: TcpConnectionCloseTotal::new(&self.0),
            tcp_connection_duration_seconds: self.0.register_histogram(
                "tcp_connection_duration_seconds",
                "Histogram of how long each connection has lived for",
            ),
            shares_total: self.0.register_generic_counter_vec(
                "shares_total",
                "Sum of shares difficulty that have been processed",
                variable_label_names,
            ),
            submits_total: self.0.register_generic_counter_vec(
                "submits_total",
                "Sum of submits that have processed",
                variable_label_names,
            ),
            v1_request_duration_seconds: self.0.register_histogram_vec(
                "v1_request_duration_seconds",
                "Histogram of duration if stratum V1 requests",
                &["type", "status"],
            ),
        })
    }

    pub fn stats_log_task(&self) {
        let cloned_registry = self.0.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                match cloned_registry.to_text() {
                    Ok((stats_buf, _)) => {
                        match std::str::from_utf8(&stats_buf) {
                            Ok(metrics_str) => info!("{}", metrics_str.replace("\n", ";")),
                            Err(e) => error!("Cannot convert metrics to string: {:?}", e),
                        };
                    }
                    Err(e) => error!("Cannot dump metrics into log: {}", e),
                }
            }
        });
    }
}

pub struct ProxyMetrics {
    /// TCP connection open events
    tcp_connection_open_total: IntCounter,
    /// TCP connection close events
    pub tcp_connection_close_stage: TcpConnectionCloseTotal,
    /// Histogram of how long each connection has lived for
    pub tcp_connection_duration_seconds: Histogram,
    /// Aggregate of submitted shares, labels:
    /// - type = (downstream, upstream)
    /// - status = (accepted, rejected)
    shares_total: IntCounterVec,
    /// Aggregate of submits => 1 submit = DIFF shares, labels:
    /// - type = (downstream, upstream)
    /// - status = (accepted, rejected)
    submits_total: IntCounterVec,

    /// V1 request duration histogram that distinguishes between individual request types
    /// - type (subscribe, authorize, submit, other)
    /// - status (success, error)
    v1_request_duration_seconds: HistogramVec,
}

impl ProxyMetrics {
    /// Helper that accounts a share if `target` is provided among timeseries specified by
    /// `label_values`. If no target is specified only submit is accounted
    fn account_share(&self, target: Option<U256>, label_values: &[&str]) {
        if let Some(tgt) = target {
            let share_value = V2ToV1Translation::target_to_diff(tgt)
                .try_into()
                .expect("BUG: Failed to convert target difficulty");
            self.shares_total
                .with_label_values(label_values)
                .inc_by(share_value);
        }
        self.submits_total.with_label_values(label_values).inc();
    }

    fn observe_v1_request_duration(
        &self,
        request_method: ii_stratum::v1::rpc::Method,
        duration: Duration,
        status: &str,
    ) {
        let request_method_name = match request_method {
            Method::Authorize => "authorize",
            Method::Configure => "configure",
            Method::Subscribe => "subscribe",
            Method::Submit => "submit",
            _ => "other",
        };
        self.v1_request_duration_seconds
            .with_label_values(&[request_method_name, status])
            .observe(duration.as_secs_f64());
    }

    pub fn account_accepted_share(&self, target: Option<U256>) {
        self.account_share(target, &["downstream", "accepted"]);
    }

    pub fn account_rejected_share(&self, target: Option<U256>) {
        self.account_share(target, &["downstream", "rejected"]);
    }

    pub fn account_opened_connection(&self) {
        self.tcp_connection_open_total.inc();
    }
    pub fn observe_v1_request_success(
        &self,
        request_method: ii_stratum::v1::rpc::Method,
        duration: Duration,
    ) {
        self.observe_v1_request_duration(request_method, duration, "success");
    }
    pub fn observe_v1_request_error(
        &self,
        request_method: ii_stratum::v1::rpc::Method,
        duration: Duration,
    ) {
        self.observe_v1_request_duration(request_method, duration, "error");
    }
    pub fn tcp_connection_timer_observe(&self, timer: Instant) {
        self.tcp_connection_duration_seconds
            .observe(timer.elapsed().as_secs_f64());
    }
    pub fn tcp_connection_close_ok(&self) {
        self.tcp_connection_close_stage.inc_as_ok()
    }
    pub fn tcp_connection_close_with_error(&self, error: &error::Error) {
        self.tcp_connection_close_stage.inc_by_error(error)
    }
}

pub trait ErrorLabeling {
    fn label(&self) -> &str;
}

impl ErrorLabeling for error::DownstreamError {
    fn label(&self) -> &str {
        match self {
            Self::EarlyIo(_) => "early",
            Self::ProxyProtocol(_) => "haproxy",
            _ => "downstream",
        }
    }
}

impl ErrorLabeling for error::UpstreamError {
    fn label(&self) -> &str {
        "upstream"
    }
}

impl ErrorLabeling for error::V2ProtocolError {
    fn label(&self) -> &str {
        match self {
            Self::SetupConnection(_) => "setup_connection",
            Self::OpenMiningChannel(_) => "open_mining_channel",
            Self::Other(_) => "protocol_other",
        }
    }
}

impl ErrorLabeling for error::Error {
    fn label(&self) -> &str {
        use ii_stratum::error::Error as StratumError;
        match self {
            Self::GeneralWithMetricsLabel(_, label) => label,
            Self::Stratum(s) => match s {
                StratumError::Noise(_)
                | StratumError::NoiseEncoding(_)
                | StratumError::NoiseProtocol(_)
                | StratumError::NoiseSignature(_) => "noise",
                StratumError::V2(_) => "downstream",
                StratumError::V1(_) => "upstream",
                _ => "stratum_other",
            },
            Self::Downstream(err) => err.label(),
            Self::Upstream(err) => err.label(),
            Self::Utf8(_) => "utf8",
            Self::Json(_) => "json",
            Self::Protocol(e) => e.label(),
            Self::General(_) => "general",
            Self::Timeout(_) => "timeout",
            Self::ClientAttempt(_) => "client_attempt",
            Self::BitcoinHashes(_) => "bitcoin_hashes",
            Self::InvalidFile(_) => "invalid_file",
            Self::Metrics(_) => "metrics",
            Self::Io(_) => "io",
        }
    }
}
