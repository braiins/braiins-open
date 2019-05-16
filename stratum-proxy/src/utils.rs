use std::future::Future as StdFuture;
use std::pin::Pin;
use futures::compat::Future01CompatExt;
use futures::compat::Compat;
use futures::{FutureExt, TryFutureExt};
use futures::future::UnitError;
use tokio::prelude::Future as TokioFuture;


/// This is a wrapper that performs some more jugglign to convert
/// 0.3 future into a 0.1 future runnable by Tokio including I/O.
///
/// It turns out Tokio's async/await preview layer is not enough
/// due to incompatibilities in task waking (I think).
/// cf. https://stackoverflow.com/questions/55447650/tokiorun-async-with-tokionetunixstream-panics/56171513
pub trait CompatFix: StdFuture {
    type TokioFuture: TokioFuture<Item=Self::Output, Error=()>;

    fn compat_fix(self) -> Self::TokioFuture;
}

impl<F: StdFuture + Send + 'static> CompatFix for F {
    type TokioFuture = Compat<Pin<Box<dyn StdFuture<Output = Result<Self::Output, ()>> + Send>>>;

    fn compat_fix(self) -> Self::TokioFuture {
        self.unit_error().boxed().compat()
    }
}
