// Copyright (C) 2019  Braiins Systems s.r.o.
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

//! Async utilities

#[cfg(all(feature = "tokio03", feature = "tokio02"))]
compile_error!("You can't use both Tokio 0.3 and 0.2. Note: The `tokio02` feature requires default features to be turned off");

#[cfg(all(feature = "tokio12", feature = "tokio02"))]
compile_error!("You can't use both Tokio 1.2 and 0.2. Note: The `tokio02` feature requires default features to be turned off");

#[cfg(all(feature = "tokio12", feature = "tokio03"))]
compile_error!("You can't use both Tokio 1.2 and 0.3. Note: The `tokio02` feature requires default features to be turned off");

#[cfg(feature = "tokio12")]
mod halthandle;
#[cfg(feature = "tokio12")]
pub use halthandle::*;

#[cfg(feature = "tokio03")]
mod halthandle03;
#[cfg(feature = "tokio03")]
pub use halthandle03::*;

#[cfg(feature = "tokio02")]
mod halthandle02;
#[cfg(feature = "tokio02")]
pub use halthandle02::*;

mod maybe_future;
pub use maybe_future::MaybeFuture;

use std::panic::{self, PanicInfo};
use std::pin::Pin;
use std::process;
use std::sync::Once;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::prelude::*;
use once_cell::sync::OnceCell;
use pin_project_lite::pin_project;
use tokio::time::{self, Instant};

/// This registers a customized panic hook with the stdlib.
/// The customized panic hook does the same thing as the default
/// panic handling - ie. it prints out the panic information
/// and optionally a trace - but then it calls abort().
///
/// This means that a panic in Tokio threadpool worker thread
/// will bring down the whole program as if the panic
/// occured on the main thread.
///
/// This function can be called any number of times,
/// but the hook will be set only on the first call.
/// This is thread-safe.
pub fn setup_panic_handling() {
    static HOOK_SETTER: Once = Once::new();

    HOOK_SETTER.call_once(|| {
        let default_hook = panic::take_hook();

        let our_hook = move |pi: &PanicInfo| {
            default_hook(pi);
            process::abort();
        };

        panic::set_hook(Box::new(our_hook));
    });
}

pin_project! {
    pub struct Cancelable<F, Fc> {
        #[pin]
        ft: F,
        #[pin]
        cancel_ft: Fc,
    }
}

impl<F, Fc> Cancelable<F, Fc>
where
    F: Future,
    Fc: Future,
{
    fn new(ft: F, cancel_ft: Fc) -> Self {
        Self { ft, cancel_ft }
    }

    pub fn into_inner(self) -> (F, Fc) {
        (self.ft, self.cancel_ft)
    }
}

impl<F, Fc> Future for Cancelable<F, Fc>
where
    F: Future,
    Fc: Future,
{
    type Output = Result<F::Output, Fc::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // First, try polling the cancel future:
        if let Poll::Ready(res) = this.cancel_ft.poll(cx) {
            return Poll::Ready(Err(res));
        }

        // Not cancelled, poll the original future:
        this.ft.poll(cx).map(Ok)
    }
}

/// An extension trait for `Future` goodies,
/// currently this only entails the `timeout()` function.
pub trait FutureExt: Future + Sized {
    /// Require a `Future` to complete before the specified duration has elapsed.
    ///
    /// This is a chainable alias for `tokio::time::timeout()`.
    fn timeout(self, timeout: Duration) -> time::Timeout<Self>
    where
        Self: Sized,
    {
        time::timeout(timeout, self)
    }

    /// Make this future cancelable: The future is cancelled
    /// when the `cancel_ft` resolves before the wrapped
    /// future itself resolves.
    ///
    /// If the original future resolves, its value is yielded as `Ok(value)`.
    /// If cancelled, `Err(e)` is yielded,
    /// where `e` is the value yielded by `cancel_ft`.
    ///
    /// This is basically the same operation as `select()`
    /// but yielding a `Result` and with a clearer intent of cancelation.
    fn cancel<Fc>(self, cancel_ft: Fc) -> Cancelable<Self, Fc>
    where
        Fc: Future,
    {
        Cancelable::new(self, cancel_ft)
    }
}

impl<F: Future> FutureExt for F {}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn timeout() {
        let timeout = Duration::from_millis(10);

        let future = future::pending::<()>().timeout(timeout);
        future.await.expect_err("BUG: Timeout expected");

        let mut stream = tokio_stream::pending::<()>();
        let future = stream.next().timeout(timeout);
        future.await.expect_err("BUG: Timeout expected");
    }

    #[tokio::test]
    async fn cancel() {
        // Verify cancelling:
        let cancel = future::ready(1);
        let fut = future::pending::<()>().cancel(cancel);
        assert_eq!(fut.await, Result::<(), u32>::Err(1));

        // Verify not cancelling:
        let cancel = future::pending::<()>();
        let fut = future::ready(2).cancel(cancel);
        assert_eq!(fut.await, Result::<u32, ()>::Ok(2));

        // Usage with Tripwire is verified in halthandle...
    }
}

/// An instance of `Instant` used as a reference/anchor for coarse-grained timer.
/// All "grainy" time instants are constructed in exactly `N * 100ms` distance from this
/// base.
static GRAINY_TIMER_BASE: OnceCell<Instant> = OnceCell::new();

/// Moves the given instant value to the first point which is on a 100ms time grid,
/// starting from `GRAINY_TIMER_BASE` instant.
/// This function never moves the instant to the past, always to the future or keeps
/// it the same in case when it is already on the grid (or this is the first ever call
/// to this function and `initialize_grainy_timer` has not been called before).
#[inline(never)]
pub fn make_grainy(instant: Instant) -> Instant {
    /// This must be on a milli-second grid. We cannot use ms directly because of
    /// "not move back" requirement.
    const PRECISION_NS: u64 = 100_000_000;
    let base = *GRAINY_TIMER_BASE.get_or_init(Instant::now);
    if instant < base {
        return instant;
    }
    let ns = instant.duration_since(base).as_nanos() as u64;
    // Move the point almost by one full grid to the right. The "one" keeps
    // any already-on-the grid value in place.
    let ns = ns + PRECISION_NS - 1;
    // Truncate the point to the first grid point to the left.
    let ns = ns - ns % PRECISION_NS;
    base + Duration::from_nanos(ns)
}

/// Makes sure we have the GRAINY_TIMER_BASE initialized. This should be ideally
/// called before first `Instance::now()` is called, which should be later used
/// in "grainy" context.
pub fn initialize_grainy_timer() {
    let _ = make_grainy(Instant::now());
}

/// Provides support for more efficient, coarse-grained timeouts for a generic futures.
pub trait GrainyTimeout: Future {
    /// Require a `Future` to complete before the specified duration has elapsed,
    /// when used a grainy deadline. The actual timeout will be equal or larger than
    /// the requested one by the `timeout` parameter.
    fn grainy_timeout(self, timeout: Duration) -> time::Timeout<Self>
    where
        Self: Sized,
    {
        // "Optimal" deadline.
        let deadline = Instant::now() + timeout;
        // Coarse-grained deadline.
        let deadline = make_grainy(deadline);
        time::timeout_at(deadline, self)
    }
}

impl<F: Future> GrainyTimeout for F {}

#[cfg(test)]
mod test2 {
    use super::{initialize_grainy_timer, make_grainy, Instant};

    #[test]
    fn grainy() {
        initialize_grainy_timer();
        // Try it few times for potentially hitting some unexpected corner case.
        for _ in 0..100 {
            // Get some instant not on the grid.
            let i = loop {
                let i = Instant::now();
                let g = make_grainy(i);
                if i != g {
                    break i;
                }
            };

            let g1 = make_grainy(i);
            let g2 = make_grainy(g1);

            assert!(i < g1);
            // It is stable
            assert_eq!(g1, g2);
        }
    }
}
