use crate::tokio::r#await;
use crate::tokio::timer::Delay;
use futures::compat::Future01CompatExt;
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
pub async fn backoff<E, T, FT: Future<Output = Result<T, E>>, F: Fn() -> FT>(
    start_delay: u32,
    iterations: u32,
    f: F,
) -> Result<T, E> {
    let mut delay = start_delay;
    let mut res = await!(f());
    if res.is_ok() {
        return res;
    }

    for i in 0..iterations {
        await!(Delay::new(Instant::now() + Duration::from_millis(delay as u64)).compat());
        delay = 2 * delay;

        res = await!(f());
        if res.is_ok() {
            return res;
        }
    }

    res
}
