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

//! **Unvariant** makes it easier to extract/parse specific Rust types
//! out of various variant-like types, such as types holding network messages etc.
//!
//! The two main tools unvariant offers are the `unvariant!()` macro and the `#[handler]`
//! attribute macro. Both macros use the `Id` and `GetId` traits.
//!
//! The `GetId` trait should be implemented for a variant type, its purpose
//! is to extract an ID (a value) from the variant type.
//! Often this is an integer value out of a network frame header, for example,
//! but it may be any `const` type.
//!
//! The `Id` trait is used to mark specific Rust types with an ID.
//! There is also a helper macro `#[id]` which derives the `Id` trait for a type.
//!
//! ## `unvariant!()`
//!
//! The `unvariant!()` macro works like a `match` statement (it is an expression).
//! It takes a variant type,
//! reads an ID from it (via the `GetId` implementation) and matches
//! that against a set of types which implement `Id`. If the ID matches
//! one of the types, the macro uses a `From<T>` implementation to convert
//! the variant type into the specific type and pass it to the relevant arm:
//!
//! ```ignore
//! unvariant!(frame {
//!     foo: Foo => {
//!         // use foo
//!     },
//!     bar: Bar => {
//!         // use bar
//!     },
//!     id: _ => {
//!         // id matched neither Foo nor Bar, do something else
//!     },
//! })
//! ```
//!
//! The macro also has a `try` variant:
//!
//! ```ignore
//! unvariant!(try frame {
//!     // ...
//! ```
//!
//! Which works the same way except `TryFrom` is used instead of `From`
//! and the conversion `Result` is passed to the arm.
//!
//! ## `#[handler]`
//!
//! The `#[handler]` attribute macro works in a similar way except
//! methods are used for handling concrete types instead of match arms.
//!
//! It is used to annotate an impl block.
//! It comes in two shapes – sync and async:
//!
//! ```ignore
//! #[handler([try] <VariantType> [trait <CustomHandlerTrait>])]
//! ```
//!
//! Syntax of async handler:
//! ```ignore
//! #[handler(async [try] <VariantType> [suffix <fn_name_suffix>])]
//! ```
//!
//! The optional `try` argument, like in `unvariant!()` is for using `TryFrom`
//! for covnersion from the variant type to a specific type.
//! In both sync and async versions a `handle()` method is defined
//! on the type which is the entry point.
//!
//! The impl block should contain one method for each specific type that
//! you wish to extract from the variant type and an error-handling/catch-all
//! function marked by `#[handle(_)]`.
//!
//! ```ignore
//! #[handler(async try Frame)]
//! impl MyHandler {
//!     async fn handle_foo(&mut self, foo: Foo) -> Result<()> {
//!         // process Foo
//!     }
//!
//!     fn handle_bar(&mut self, bar: Bar) -> Result<()> {
//!         // process Bar
//!     }
//!
//!     #[handle(_)]
//!     fn handle_unknown(&mut self, frame: Result<Frame, Error>) -> Result<u32, u32> {
//!         match frame {
//!             Ok(frame) => { /* The ID didn't match any types */ }
//!             Err(err) => { /* The TryFrom conversion failed */ }
//!         }
//!     }
//! }
//! ```
//!
//! The optional `suffix` argument is used to specify a name suffix
//! for the functions that the macro generates, this is useful when
//! you need to implement two different handlers on the same type.
//! Ie. one handler block can use `suffix _a` (a `handle_a()`
//! method will be generated), and the other block can use `suffix _b`
//! (a `handle_b()` method will be generated).

use std::pin::Pin;

use proc_macro_hack::proc_macro_hack;

pub use ii_unvariant_macros::{handler, id};

#[proc_macro_hack]
pub use ii_unvariant_macros::unvariant;

mod macro_private;
pub use macro_private::*;

/// Trait that must be implemented by types handled by unvariant handler.
/// This can be derived by procedural macro `#[id(...)]`
pub trait Id<T: Copy> {
    const ID: T;
}

/// Shorthand implementation of `Id` for a type or multiple types
/// for when you can't use the `#[id(...)]` macro.
#[macro_export]
macro_rules! id_for {
    ($id_ty:ty, $($for_ty:ty => $id:expr),+ $(,)*) => {
        $(impl ::ii_unvariant::Id<$id_ty> for $for_ty {
            const ID: $id_ty = $id;
        })+
    };
}

/// Trait that must be implemented for frames being handled by unvariant handler.
pub trait GetId {
    type Id;

    fn get_id(&self) -> Self::Id;
}

impl<'a, T: GetId> GetId for &'a T {
    type Id = T::Id;

    fn get_id(&self) -> Self::Id {
        GetId::get_id(*self)
    }
}

impl<'a> GetId for &'a str {
    type Id = &'a str;

    fn get_id(&self) -> &'a str {
        self
    }
}

impl<'a> GetId for &'a String {
    type Id = &'a str;

    fn get_id(&self) -> &'a str {
        self.as_str()
    }
}

pub trait Handler<T: GetId, O> {
    fn handle(&mut self, variant: T) -> O;
}

pub struct AsyncHandler<T, O> {
    ff: Pin<Box<dyn FilterFuture<T, Output = O>>>,
}

impl<T, O> AsyncHandler<T, O>
where
    T: GetId,
{
    pub async fn handle(&mut self, variant: T) -> O {
        self.ff.as_mut().input(variant);
        self.ff.as_mut().await
    }
}
