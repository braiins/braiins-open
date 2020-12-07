use std::future::Future;
use std::pin::Pin;

/// Object representing a possible future with `Output=T` or a result directly.
/// Internally a boxed future is used to seal the returning future type.
///
/// The main intention is to use this future in async trait context when
/// a called function can but mustn't have to need to be async.
/// Traditionally a user has to pay for the async call by boxing the result
/// future for all calls. `MaybeFuture` allows to pay the allocation price
/// only when it is needed.
#[must_use = "this `MaybeFuture` may represent a future, it must be awaited!"]
pub struct MaybeFuture<T>(MaybeFutureInner<T>);

/// Internal implementation of `MaybeFuture`. This is non-public type,
/// preventing invalid construction.
enum MaybeFutureInner<T> {
    Future(Pin<Box<dyn Future<Output = T> + Send>>),
    Ready(Option<T>),
}

// TODO: Replace by a proper implementation of `std::futures::Future`
impl<T> MaybeFuture<T> {
    /// This must be called exactly once.
    pub async fn value(&mut self) -> T {
        match &mut self.0 {
            MaybeFutureInner::Ready(val) => val.take().expect("BUG: Internal consistency breached"),
            MaybeFutureInner::Future(bx) => bx.await,
        }
    }
}

impl<T> MaybeFuture<T> {
    pub fn future<F: Future<Output = T> + Send + 'static>(fut: F) -> Self {
        Self(MaybeFutureInner::Future(Box::pin(fut)))
    }
    pub fn result(val: T) -> Self {
        Self(MaybeFutureInner::Ready(Some(val)))
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
