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

use crate::tokio::timer;
use futures::future::Future;
use std::time::{Duration, Instant};

/// Run an async function/lambda repeatedly with backoff until it
/// returns Ok(...) or until the number of inerations is reached.
///
/// `start_delay` is the starting timeout in milliseconds, `iterations`
/// is the maximum number of re-tries. The delay is doubled in each iteration.
///
/// The last `Result` from the callback function is returned,
/// carrying either an Ok value or an error.
/// TODO: review tokio-retry if that would be a suitable implementation instead of a custom one
/// NB. tokio-retry seems to not be updated for tokio 0.2
pub async fn backoff<E, T, FT: Future<Output = Result<T, E>>, F: Fn() -> FT>(
    start_delay: u32,
    iterations: u32,
    f: F,
) -> Result<T, E> {
    let mut delay = start_delay;
    let mut res = f().await;
    if res.is_ok() {
        return res;
    }

    for _ in 0..iterations {
        timer::delay_for(Duration::from_millis(delay as u64)).await;
        delay = 2 * delay;

        res = f().await;
        if res.is_ok() {
            return res;
        }
    }

    res
}
