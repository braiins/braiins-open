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
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::parse_macro_input;

use std::io::{Error, ErrorKind, Result};
use std::process::Command;

macro_rules! error {
	($($args:tt)*) => {
		syn::Error::new(Span::call_site(), format!($($args)*))
	};
}

#[derive(Default)]
struct GitHashInput {
    object: Option<String>,
    length: Option<usize>,
}

impl Parse for GitHashInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut result = GitHashInput::default();

        while !input.is_empty() {
            let arg: syn::Ident = input.parse()?;
            let _: syn::token::Eq = input.parse()?;
            match arg.to_string().as_str() {
                "object" => {
                    let name: syn::LitStr = input.parse()?;
                    result.object.replace(name.value());
                }
                "length" => {
                    let value: syn::LitInt = input.parse()?;
                    result.length.replace(value.base10_parse()?);
                }
                name => {
                    return Err(error!("Unexpected argument `{}`", name));
                }
            }
            if !input.is_empty() {
                let _: syn::token::Comma = input.parse()?;
            }
        }

        Ok(result)
    }
}

fn get_git_hash(
    git_executable_path: &str,
    default_var: &str,
    input: GitHashInput,
) -> Result<String> {
    let object = input.object.as_deref().unwrap_or("HEAD");
    Command::new(git_executable_path)
        .arg("rev-parse")
        .arg(object)
        .output()
        .and_then(|output| {
            if output.status.success() {
                let output = String::from_utf8_lossy(&output.stdout);
                let mut hash = output.trim_end().to_string();
                if let Some(length) = input.length {
                    hash.truncate(length);
                }
                Ok(hash)
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Invalid git revision requested".to_string(),
                ))
            }
        })
        .or_else(|_| {
            std::env::var(default_var).map_err(|e| Error::new(ErrorKind::Other, e.to_string()))
        })
}

fn impl_git_hash(input: GitHashInput) -> syn::Result<proc_macro2::TokenStream> {
    match get_git_hash("git", "GIT_HASH", input) {
        Ok(hash) => Ok(quote!(#hash)),
        Err(e) => Err(error!("{}", e)),
    }
}

#[proc_macro]
pub fn git_hash(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as GitHashInput);

    let tokens = match impl_git_hash(args) {
        Ok(x) => x,
        Err(e) => e.to_compile_error(),
    };

    TokenStream::from(tokens)
}

#[cfg(test)]
mod tests {
    use super::get_git_hash;
    use crate::GitHashInput;

    #[test]
    fn failing_git_command() {
        get_git_hash(
            "git",
            "CARGO_PKG_NAME",
            GitHashInput {
                object: Some("HEAD".to_string()),
                length: None,
            },
        )
        .expect("BUG: Failed to extract revision number from git");

        get_git_hash(
            "some-non-existing-command",
            "CARGO_PKG_NAME",
            GitHashInput {
                object: Some("HEAD".to_string()),
                length: None,
            },
        )
        .expect("BUG: Failed to extract revision number from variable");

        get_git_hash(
            "some-non-existing-command",
            "SOME_NONEXISTING_VARIABLE",
            GitHashInput {
                object: Some("HEAD".to_string()),
                length: None,
            },
        )
        .expect_err("BUG: Failed to fail to extract version");
    }
}
