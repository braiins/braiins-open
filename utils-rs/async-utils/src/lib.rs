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

#[cfg(feature = "tokio03")]
mod halthandle;
#[cfg(feature = "tokio03")]
pub use halthandle::*;

#[cfg(feature = "tokio02")]
mod halthandle02;
#[cfg(feature = "tokio02")]
pub use halthandle02::*;

use std::panic::{self, PanicInfo};
use std::process;
use std::sync::Once;
use std::time::Duration;

use futures::prelude::*;
use tokio::time;

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

/// An extension trait for `Future` goodies,
/// currently this only entails the `timeout()` function.
pub trait FutureExt: Future {
    /// Require a `Future` to complete before the specified duration has elapsed.
    ///
    /// This is a chainable alias for `tokio::time::timeout()`.
    fn timeout(self, timeout: Duration) -> time::Timeout<Self>
    where
        Self: Sized,
    {
        time::timeout(timeout, self)
    }
}

impl<F: Future> FutureExt for F {}

#[cfg(test)]
mod test {
    use super::*;

    use tokio::stream;

    #[tokio::test]
    async fn timeout() {
        let timeout = Duration::from_millis(100);

        let future = future::pending::<()>().timeout(timeout);
        future.await.expect_err("BUG: Timeout expected");

        let mut stream = stream::pending::<()>();
        let future = stream.next().timeout(timeout);
        future.await.expect_err("BUG: Timeout expected");
    }
}
