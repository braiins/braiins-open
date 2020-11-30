use std::fmt::Formatter;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering::*},
    Arc,
};
use tokio::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct MetricsSnapshot {
    pub total_connections_opened: u64,
    pub windowed_connections_opened: u64,
    pub windowed_connections_closed: u64,
    pub total_connections_closed: u64,
    pub total_accepted_shares: u64,
    pub windowed_accepted_shares: u64,
}

#[derive(Default)]
pub struct Metrics {
    total_connections_opened: AtomicU64,
    windowed_connections_opened: AtomicU64,
    windowed_connections_closed: AtomicU64,
    total_connections_closed: AtomicU64,
    total_accepted_shares: AtomicU64,
    windowed_accepted_shares: AtomicU64,
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
            "new_accepted_shares": self.total_accepted_shares,
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
            total_connections_closed: self.total_connections_closed.load(Relaxed),
        }
    }

    pub fn account_share(&self) {
        self.total_accepted_shares.fetch_add(1, Relaxed);
    }
    pub fn account_opened_connection(&self) {
        self.total_connections_opened.fetch_add(1, Relaxed);
    }
    pub fn account_closed_connection(&self) {
        self.total_connections_closed.fetch_add(1, Relaxed);
    }

    pub fn spawn_stats(self: Arc<Self>) {
        tokio::spawn(async move {
            let this = self.as_ref();

            let mut shares_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            let mut opened_connections_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            let mut closed_connections_buf = VecDeque::with_capacity(Self::TIME_WINDOW_LENGTH);
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                this.update_share_window(&mut shares_buf);
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

    fn update_share_window(&self, buf: &mut VecDeque<u64>) {
        let window_end = self.total_accepted_shares.load(Relaxed);
        buf.push_back(window_end);
        if buf.len() == Self::TIME_WINDOW_LENGTH {
            buf.pop_front();
        }
        let window_beginning = buf.front().cloned().unwrap_or_default();
        self.windowed_accepted_shares
            .store(window_end - window_beginning, Relaxed);
    }
}
