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

//! This module implements server controller that controls the way new connections are accepted
//! and graceful shutdown

use std::sync::{
    atomic::{AtomicUsize, Ordering::Relaxed},
    Arc, Mutex,
};
use std::task::{Context, Poll, Waker};

use ii_async_utils::FutureExt;
use ii_logging::macros::*;
use tokio::sync::Notify;
use tokio::time::Duration;

#[derive(Default)]
pub struct ClientCounter {
    client_counter: Arc<AtomicUsize>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl Clone for ClientCounter {
    fn clone(&self) -> Self {
        let client_counter = self.client_counter.clone();
        client_counter.fetch_add(1, Relaxed);
        Self {
            client_counter,
            waker: self.waker.clone(),
        }
    }
}

/// Future completes as soon as client counter is decreased to 0, signalling that there are no
/// clients connected to the server
impl std::future::Future for ClientCounter {
    type Output = ();

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.client_counter.load(Relaxed) == 0 {
            Poll::Ready(())
        } else {
            let mut waker_grd = self.waker.lock().expect("BUG: Poisoned client counter");
            waker_grd.replace(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl ClientCounter {
    /// Decreases internal client counter and wakes its future if it hits 0
    pub fn decrease(&mut self) {
        assert_ne!(
            self.client_counter.fetch_sub(1, Relaxed),
            0,
            "BUG: Client counter underflow"
        );
        // If this is the last client, wake its future
        if self.client_counter.load(Relaxed) == 0 {
            let waker_opt = self
                .waker
                .lock()
                .expect("BUG: Poisoned client counter")
                .take();
            if let Some(waker) = waker_opt {
                waker.wake()
            }
        }
    }
}

/// Tells whether controller should wait until all clients disconnects or whether the
/// wait_for_termination method should return immediately
#[derive(Copy, Clone)]
pub enum TerminationMethod {
    LazyTermination,
    ImmediateTermination,
}

impl Default for TerminationMethod {
    fn default() -> Self {
        Self::LazyTermination
    }
}

/// Structure that tracks amount of clients connected to the server.
///
/// If the server is about to be shut down, it allows to wait some time:
/// 1. if Immediate termination is requested, function returns immediately
/// 2. If Lazy termination is requested, function returns after number of client is zero
/// or after timeout elapses, if specified.
#[derive(Default)]
pub struct Controller {
    client_counter: ClientCounter,
    termination_method: TerminationMethod,
    termination_notifier: Arc<Notify>,
}

impl Controller {
    pub async fn wait_for_termination(self, timeout: Option<Duration>) {
        use TerminationMethod::*;
        match self.termination_method {
            ImmediateTermination => {}
            LazyTermination => {
                if let Some(timeout) = timeout {
                    if let Err(_) = self.client_counter.timeout(timeout).await {
                        info!("Graceful period for termination timed out")
                    }
                } else {
                    self.client_counter.await;
                }
                info!("Terminating proxy");
            }
        }
    }

    /// Returns notifier that may be used to release  [`wait_for_notification`] method
    pub fn termination_notifier(&self) -> Arc<Notify> {
        self.termination_notifier.clone()
    }

    pub async fn wait_for_notification(&self) {
        self.termination_notifier.notified().await;
    }

    pub fn request_immediate_termination(&mut self) {
        self.termination_method = TerminationMethod::ImmediateTermination;
    }

    /// Returns ClientCounter structure and increments
    pub fn counter_for_new_client(&self) -> ClientCounter {
        self.client_counter.clone()
    }
}
