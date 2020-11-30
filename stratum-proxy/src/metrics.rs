use crate::translation::V2ToV1Translation;
use primitive_types::U256;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::fmt::Formatter;
use std::sync::{
    atomic::{AtomicU64, Ordering::*},
    Arc,
};
use tokio::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct MetricsSnapshot {
    total_connections_opened: u64,
    windowed_connections_opened: u64,
    windowed_connections_closed: u64,
    total_connections_closed: u64,

    total_accepted_shares: u64,
    windowed_accepted_shares: u64,

    total_rejected_shares: u64,
    windowed_rejected_shares: u64,

    // total_stale_shares: u64,
    // windowed_stale_shares: u64,
    total_accepted_submits: u64,
    windowed_accepted_submits: u64,

    total_rejected_submits: u64,
    windowed_rejected_submits: u64,
    // total_stale_submits: u64,
    // windowed_stale_submits: u64,
}

#[derive(Default)]
pub struct Metrics {
    total_connections_opened: AtomicU64,
    windowed_connections_opened: AtomicU64,
    windowed_connections_closed: AtomicU64,
    total_connections_closed: AtomicU64,

    total_accepted_shares: AtomicU64,
    windowed_accepted_shares: AtomicU64,

    total_rejected_shares: AtomicU64,
    windowed_rejected_shares: AtomicU64,

    // total_stale_shares: AtomicU64,
    // windowed_stale_shares: AtomicU64,
    total_accepted_submits: AtomicU64,
    windowed_accepted_submits: AtomicU64,

    total_rejected_submits: AtomicU64,
    windowed_rejected_submits: AtomicU64,
    // total_stale_submits: AtomicU64,
    // windowed_stale_submits: AtomicU64,
}

impl std::fmt::Display for MetricsSnapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let currently_open = self
            .total_connections_opened
            .saturating_sub(self.total_connections_closed);

        let stats_output = serde_json::json!({
            "time_window_length": Metrics::TIME_WINDOW_LENGTH,
            "new_connections_opened": self.windowed_connections_opened,
            "new_connections_closed": self.windowed_connections_closed,
            "new_accepted_shares": self.windowed_accepted_shares,
            "total_accepted_shares": self.total_accepted_shares,
            "new_rejected_shares": self.windowed_rejected_shares,
            "total_rejected_shares": self.total_rejected_shares,
            "new_accepted_submits": self.windowed_accepted_submits,
            "total_accepted_submits": self.total_accepted_submits,
            "new_rejected_submits": self.windowed_rejected_submits,
            "total_rejected_submits": self.total_rejected_submits,
            "currently_open_connections": currently_open,
        });
        let snapshot = serde_json::to_string_pretty(&stats_output).unwrap_or_else(|_| "{}".into());
        write!(f, "{}", snapshot)
    }
}

impl Metrics {
    const TIME_WINDOW_LENGTH: usize = 60;

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_connections_opened: self.total_connections_opened.load(Relaxed),
            windowed_connections_opened: self.windowed_connections_opened.load(Relaxed),
            windowed_connections_closed: self.windowed_connections_closed.load(Relaxed),

            windowed_accepted_shares: self.windowed_accepted_shares.load(Relaxed),
            total_accepted_shares: self.total_accepted_shares.load(Relaxed),

            windowed_rejected_shares: self.windowed_rejected_shares.load(Relaxed),
            total_rejected_shares: self.total_rejected_shares.load(Relaxed),

            windowed_accepted_submits: self.windowed_accepted_submits.load(Relaxed),
            total_accepted_submits: self.total_accepted_submits.load(Relaxed),

            windowed_rejected_submits: self.windowed_rejected_submits.load(Relaxed),
            total_rejected_submits: self.total_rejected_submits.load(Relaxed),

            total_connections_closed: self.total_connections_closed.load(Relaxed),
        }
    }

    pub fn account_accepted_share(&self, target: Option<U256>) {
        if let Some(tgt) = target {
            let share_value = V2ToV1Translation::target_to_diff(tgt)
                .try_into()
                .expect("BUG: Failed to convert target difficulty");
            self.total_accepted_shares.fetch_add(share_value, Relaxed);
        }
        self.total_accepted_submits.fetch_add(1, Relaxed);
    }

    pub fn account_rejected_share(&self, target: Option<U256>) {
        if let Some(tgt) = target {
            let share_value = V2ToV1Translation::target_to_diff(tgt)
                .try_into()
                .expect("BUG: Failed to convert target difficulty");
            self.total_rejected_shares.fetch_add(share_value, Relaxed);
        }
        self.total_rejected_submits.fetch_add(1, Relaxed);
    }

    // pub fn account_stale_share(&self, target: Option<U256>) {
    //     if let Some(tgt) = target {
    //         let share_value = V2ToV1Translation::target_to_diff(tgt).try_into()
    //             .expect("BUG: Failed to convert target difficulty");
    //         self.total_stale_shares.fetch_add(share_value, Relaxed);
    //     }
    //     self.total_stale_submits.fetch_add(1, Relaxed);
    // }

    pub fn account_opened_connection(&self) {
        self.total_connections_opened.fetch_add(1, Relaxed);
    }
    pub fn account_closed_connection(&self) {
        self.total_connections_closed.fetch_add(1, Relaxed);
    }

    pub fn spawn_stats(self: Arc<Self>) {
        tokio::spawn(async move {
            let this = self.as_ref();

            let mut accepted_shares_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            let mut rejected_shares_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            // let mut stale_shares_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            let mut opened_connections_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            let mut closed_connections_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                this.update_accepted_share_window(&mut accepted_shares_buf);
                this.update_rejected_share_window(&mut rejected_shares_buf);
                // this.update_stale_share_window(&mut stale_shares_buf);
                this.update_opened_connection_window(&mut opened_connections_buf);
                this.update_closed_connection_window(&mut closed_connections_buf);
            }
        });
    }

    fn update_opened_connection_window(&self, buf: &mut VecDeque<u64>) {
        let window_end = self.total_connections_opened.load(Relaxed);
        buf.push_back(window_end);
        if buf.len() == Self::TIME_WINDOW_LENGTH {
            buf.pop_front();
        }
        let window_beginning = buf.front().cloned().unwrap_or_default();
        self.windowed_connections_opened
            .store(window_end - window_beginning, Relaxed);
    }

    fn update_closed_connection_window(&self, buf: &mut VecDeque<u64>) {
        let window_end = self.total_connections_closed.load(Relaxed);
        buf.push_back(window_end);
        if buf.len() == Self::TIME_WINDOW_LENGTH {
            buf.pop_front();
        }
        let window_beginning = buf.front().cloned().unwrap_or_default();
        self.windowed_connections_closed
            .store(window_end - window_beginning, Relaxed);
    }

    fn update_accepted_share_window(&self, buf: &mut VecDeque<u64>) {
        let window_end = self.total_accepted_shares.load(Relaxed);
        buf.push_back(window_end);
        if buf.len() == Self::TIME_WINDOW_LENGTH {
            buf.pop_front();
        }
        let window_beginning = buf.front().cloned().unwrap_or_default();
        self.windowed_accepted_shares
            .store(window_end - window_beginning, Relaxed);
    }

    fn update_rejected_share_window(&self, buf: &mut VecDeque<u64>) {
        let window_end = self.total_rejected_shares.load(Relaxed);
        buf.push_back(window_end);
        if buf.len() == Self::TIME_WINDOW_LENGTH {
            buf.pop_front();
        }
        let window_beginning = buf.front().cloned().unwrap_or_default();
        self.windowed_rejected_shares
            .store(window_end - window_beginning, Relaxed);
    }

    // fn update_stale_share_window(&self, buf: &mut VecDeque<u64>) {
    //     let window_end = self.total_stale_shares.load(Relaxed);
    //     buf.push_back(window_end);
    //     if buf.len() == Self::TIME_WINDOW_LENGTH {
    //         buf.pop_front();
    //     }
    //     let window_beginning = buf.front().cloned().unwrap_or_default();
    //     self.windowed_stale_shares
    //         .store(window_end - window_beginning, Relaxed);
    // }
}
