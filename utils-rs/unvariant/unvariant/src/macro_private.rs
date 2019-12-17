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

//! These are types / functions that the macro-generated code
//! needs to function. They need to be made public in order
//! for the generated code to use them, but 'logically' they are private.
//! Please don't use them other than through the macro code.

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use super::{AsyncHandler, GetId};

/// Don't use this type directly, it's not part of the public API.
///
/// A filter future that can reset to take a new input.
/// Kind of like lockstep Sink + Stream.
#[doc(hidden)]
pub trait FilterFuture<T>: Future {
    fn input(self: Pin<&mut Self>, value: T);
}

/// Don't use this type directly, it's not part of the public API.
#[doc(hidden)]
#[pin_project]
pub struct HandlerFilter<H, Hf, F, T, O> {
    handler: H,
    handle_fn: Hf,
    variant: Option<T>,
    #[pin]
    state: Option<F>,
    _marker: PhantomData<fn() -> O>,
}

impl<H, Hf, F, T, O> HandlerFilter<H, Hf, F, T, O>
where
    F: Future<Output = O>,
    Hf: 'static + Fn(*mut H, T) -> F,
{
    /// Don't use this function, it's not part of the public API.
    pub fn __new(handler: H, handle_fn: Hf) -> Self {
        Self {
            handler,
            handle_fn,
            variant: None,
            state: None,
            _marker: PhantomData,
        }
    }
}

impl<H, Hf, F, T, O> Future for HandlerFilter<H, Hf, F, T, O>
where
    F: Future<Output = O>,
    Hf: 'static + Fn(*mut H, T) -> F,
{
    type Output = O;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<O> {
        let mut this = self.project();

        if this.state.is_none() {
            // No future in self.state, generate a new one:
            let variant = this
                .variant
                .take()
                .expect("BUG: unvariant: HandlerFilter: Neither future nor variant in state");
            let ft = (this.handle_fn)(this.handler as *mut _, variant);
            this.state.set(Some(ft));
        }

        // At this point there must be a future in self.state
        let res = this
            .state
            .as_mut()
            .as_pin_mut()
            .expect("BUG: unvariant: HandlerFilter: No Future in state")
            .poll(cx);

        res
    }
}

impl<H, Hf, F, T, O> FilterFuture<T> for HandlerFilter<H, Hf, F, T, O>
where
    F: Future<Output = O>,
    Hf: 'static + Fn(*mut H, T) -> F,
{
    fn input(self: Pin<&mut Self>, variant: T) {
        let mut this = self.project();
        this.state.set(None);
        *this.variant = Some(variant);
    }
}

impl<T, O> AsyncHandler<T, O>
where
    T: GetId,
{
    #[doc(hidden)]
    /// Do not use this function, it's not a part of the public API.
    pub fn __new<FF>(ff: FF) -> Self
    where
        FF: FilterFuture<T, Output = O> + 'static,
    {
        let ff = Box::pin(ff);
        Self { ff }
    }
}
