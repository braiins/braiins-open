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

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

mod extensions;
mod handler;
mod id;
mod unvariant;

#[proc_macro_attribute]
pub fn id(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as id::Args);
    let item = parse_macro_input!(input as id::Item);
    id::expand(args, item).into()
}

// Note: We can't use a regular proc_macro here because that can't yet be used
// in an expression context. And so dtolnay's proc_macro_hack is used here instead.
// Cf. https://github.com/rust-lang/rust/issues/54727
// This is to be straightened up once stable rust supports proc_macros in expr context.
#[proc_macro]
pub fn unvariant(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as unvariant::Item);
    unvariant::expand(item).into()
}

#[proc_macro_attribute]
pub fn handler(mut args: TokenStream, input: TokenStream) -> TokenStream {
    // Hack: args and input are merged here because we need the info
    // from args in the impl parser. An `@` token is used as a separator.
    let at = quote!(@);
    let at: TokenStream = at.into();
    args.extend(at);
    args.extend(input);
    let input = args;
    let item = parse_macro_input!(input as handler::Item);

    handler::expand(item).into()
}
