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
//! Empty metrics for the case when stratum proxy is compiled with prometheus metrics disabled

use ii_stratum::v1::rpc::Method;
pub use primitive_types::U256;
use std::time::Instant;
use tokio::time::Duration;

pub struct ProxyMetrics;

impl ProxyMetrics {
    pub fn account_accepted_share(&self, _target: Option<U256>) {}

    pub fn account_rejected_share(&self, _target: Option<U256>) {}

    pub fn account_opened_connection(&self) {}

    pub fn observe_v1_request_success(&self, _request_method: Method, _duration: Duration) {}

    pub fn observe_v1_request_error(&self, _request_method: Method, _duration: Duration) {}

    pub fn tcp_connection_timer_observe(&self, _timer: Instant) {}

    pub fn tcp_connection_close_ok(&self) {}

    pub fn tcp_connection_close_with_error(&self, _error: &crate::error::Error) {}

    pub fn accounted_spawn<T>(
        self: &std::sync::Arc<Self>,
        future: T,
    ) -> tokio::task::JoinHandle<T::Output>
    where
        T: std::future::Future + Send + 'static,
        T::Output: Send + 'static,
    {
        tokio::spawn(future)
    }
}
