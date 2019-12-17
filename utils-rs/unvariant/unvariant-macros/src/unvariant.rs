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
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{braced, Expr, Ident, Result, Token, Type};

pub enum ArmPattern {
    Type(Type),
    CatchAll,
}

pub struct Arm {
    ident: Ident,
    pat: ArmPattern,
    body: Expr,
}

impl Parse for Arm {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        let pat = if input.peek(Token![_]) {
            input.parse::<Token![_]>()?;
            ArmPattern::CatchAll
        } else {
            let ty: Type = input.parse()?;
            ArmPattern::Type(ty)
        };

        let _arrow: Token![=>] = input.parse()?;
        let body: Expr = input.parse()?;

        Ok(Self { ident, pat, body })
    }
}

pub struct Item {
    variant: Ident,
    use_try: bool,
    arms: Punctuated<Arm, Token![,]>,
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self> {
        let use_try = input.peek(Token![try]);
        if use_try {
            input.parse::<Token![try]>()?;
        }

        let variant: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let arms = content.parse_terminated(Arm::parse)?;

        Ok(Self {
            variant,
            use_try,
            arms,
        })
    }
}

struct ArmTokenizer<'a> {
    arm: &'a Arm,
    item: &'a Item,
}

impl<'a> ToTokens for ArmTokenizer<'a> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident = &self.arm.ident;
        let variant = &self.item.variant;
        let body = &self.arm.body;

        let tt = match &self.arm.pat {
            ArmPattern::Type(ty) => {
                if self.item.use_try {
                    quote!(<#ty as ::ii_unvariant::Id<_>>::ID => {
                        let #ident = <#ty as ::core::convert::TryFrom::<_>>::try_from(#variant); #body
                    })
                } else {
                    quote!(<#ty as ::ii_unvariant::Id<_>>::ID => {
                        let #ident = <#ty as ::core::convert::From::<_>>::from(#variant); #body
                    })
                }
            }
            ArmPattern::CatchAll => {
                quote!(__unvariant_catch_all_id => { let #ident = __unvariant_catch_all_id; #body })
            }
        };

        tokens.extend(tt);
    }
}

impl ToTokens for Item {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let variant = &self.variant;
        let arms: Punctuated<ArmTokenizer, Token![,]> = self
            .arms
            .iter()
            .map(|arm| ArmTokenizer { arm, item: self })
            .collect();

        tokens.extend(quote!(
            match ::ii_unvariant::GetId::get_id(&#variant) {
                #arms
            }
        ));
    }
}

pub fn expand(item: Item) -> TokenStream {
    quote!(#item)
}
