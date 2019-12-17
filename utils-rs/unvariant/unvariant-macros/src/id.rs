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

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::discouraged::Speculative;
use syn::parse::{Parse, ParseStream};
use syn::{parse_str, Expr, Generics, Ident, ItemEnum, ItemStruct, Lit, Result, Token, Type};

pub struct Args {
    pub id: Expr,
    pub ty: Type,
}

impl Args {
    fn int_suffix_to_type(suffix: &str) -> Option<Type> {
        if suffix.is_empty() {
            Some(Type::Verbatim(quote!(u32)))
        } else {
            parse_str(suffix).ok()
        }
    }

    fn float_suffix_to_type(suffix: &str) -> Option<Type> {
        if suffix.is_empty() {
            Some(Type::Verbatim(quote!(f32)))
        } else {
            parse_str(suffix).ok()
        }
    }

    fn infer_type(id: &Expr) -> Option<Type> {
        match &id {
            Expr::Lit(lit) => {
                let ty = match &lit.lit {
                    Lit::Int(i) => return Self::int_suffix_to_type(i.suffix()),
                    Lit::Str(_) => quote!(&'static str),
                    Lit::ByteStr(_) => quote!(&'static [u8]),
                    Lit::Byte(_) => quote!(u8),
                    Lit::Char(_) => quote!(char),
                    Lit::Float(f) => return Self::float_suffix_to_type(f.suffix()),
                    Lit::Bool(_) => quote!(bool),
                    Lit::Verbatim(_) => return None,
                };

                Some(Type::Verbatim(ty))
            }
            _ => None,
        }
    }
}

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        let id: Expr = input.parse()?;

        if input.is_empty() {
            let ty = match Args::infer_type(&id) {
                Some(ty) => ty,
                None => {
                    return Err(input.error("Could not infer ID type. Please specify it explicitly using the #[id(<expr> type <type>)] syntax."))
                }
            };

            Ok(Args { id, ty })
        } else {
            input.parse::<Token![type]>()?;
            let ty: Type = input.parse()?;

            Ok(Args { id, ty })
        }
    }
}

pub enum Item {
    Struct(ItemStruct),
    Enum(ItemEnum),
}

impl Item {
    fn decl(&self) -> (&Ident, &Generics) {
        match self {
            Self::Struct(s) => (&s.ident, &s.generics),
            Self::Enum(e) => (&e.ident, &e.generics),
        }
    }
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self> {
        let fork = input.fork();

        // Try to first parse a struct in a speculative manner.
        // If that fails, go back (drop fork) and try parsing an anum.
        // This might be somewhat inefficient for enums or  erroneous usages,
        // but there isn't really any other way - we can't just peek for a `struct` token,
        // because there might be an arbitrary number of arbitrarily complex
        // attributes in front of the `struct` token.
        // Note that doc comments also generate attributes.

        if let Ok(item_struct) = fork.parse::<ItemStruct>() {
            input.advance_to(&fork);
            Ok(Item::Struct(item_struct))
        } else if let Ok(item_struct) = input.parse::<ItemEnum>() {
            Ok(Item::Enum(item_struct))
        } else {
            Err(input.error("#[id(..)] may only be used with structs and enums"))
        }
    }
}

impl ToTokens for Item {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Item::Struct(item) => item.to_tokens(tokens),
            Item::Enum(item) => item.to_tokens(tokens),
        }
    }
}

pub fn expand(args: Args, item: Item) -> TokenStream {
    let (id, ty) = (args.id, args.ty);
    let (name, generics) = item.decl();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote!(
        #item

        impl #impl_generics ::ii_unvariant::Id<#ty> for #name #ty_generics #where_clause {
            const ID: #ty = #id;
        }
    )
}
