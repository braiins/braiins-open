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

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

pin_project! {
    #[doc = "Object representing a possible future with `Output=T` or a result directly.
Internally a boxed future is used to seal the returning future type.

The main intention is to use this future in async trait context when
a called function can but mustn't have to need to be async.
Traditionally a user has to pay for the async call by boxing the result
future for all calls. `MaybeFuture` allows to pay the allocation price
only when it is needed."]
#[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct MaybeFuture<T> {
        #[pin]
        inner: MaybeFutureInner<T>
    }
}

pin_project! {
    #[doc = "Internal implementation of `MaybeFuture`. This is non-public type,
preventing invalid construction."]
    #[project = InnerProjection]
    enum MaybeFutureInner<T> {
        Future { #[pin] future: Pin<Box<dyn Future<Output = T> + Send>> },
        Ready { value: Option<T> },
    }
}

impl<T> MaybeFuture<T> {
    pub fn future<F: Future<Output = T> + Send + 'static>(fut: F) -> Self {
        let inner = MaybeFutureInner::Future {
            future: Box::pin(fut),
        };
        Self { inner }
    }
    pub fn result(val: T) -> Self {
        let inner = MaybeFutureInner::Ready { value: Some(val) };
        Self { inner }
    }
}

impl<T> Future for MaybeFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Peel off the Pin layers:
        let proj = self.project().inner.project();

        // Forward to future or yield value immediately:
        match proj {
            InnerProjection::Future { future } => future.poll(cx),
            InnerProjection::Ready { value } => value
                .take()
                .expect("BUG: MaybeFuture polled after yielding Ready")
                .into(),
        }
    }
}

#[macro_export]
macro_rules! maybe {
    ($expr:expr) => {
        match $expr {
            ::std::result::Result::Ok(val) => val,
            ::std::result::Result::Err(err) => {
                return $crate::MaybeFuture::result(::std::result::Result::Err(err.into()));
            }
        }
    };
}

#[cfg(test)]
mod test {
    use super::*;

    use futures::future;
    #[cfg(feature = "tokio02")]
    use tokio02 as tokio;
    #[cfg(feature = "tokio12")]
    use tokio12 as tokio;

    #[tokio::test]
    async fn maybe_future() {
        // Test with a Future:
        let mut i = 0u32;
        let ft = future::poll_fn(move |cx| {
            if i < 42 {
                i += 1;
                cx.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(i)
            }
        });

        let maybe = MaybeFuture::future(ft);
        assert_eq!(maybe.await, 42);

        // Test with an immediate value:
        let maybe = MaybeFuture::result(42);
        assert_eq!(maybe.await, 42);
    }
}
