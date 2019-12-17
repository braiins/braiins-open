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

//! Extensions for `syn` types that are needed for macro implementation

use proc_macro2::Span;
use quote::{quote, ToTokens};
use syn::spanned::Spanned;
use syn::{
    AngleBracketedGenericArguments, Error, FnArg, GenericArgument, ImplItem, ImplItemMethod,
    Lifetime, ParenthesizedGenericArguments, PathArguments, Result, ReturnType, Token, Type,
};

use crate::unvariant::ArmPattern;

pub const HANDLE_ATTR: &str = "handle";

pub trait IsStatic {
    fn is_static(&self) -> Result<bool>;
}

fn variant_type_error<R>(span: Span) -> Result<R> {
    Err(Error::new(
        span,
        "unvariant handler: This kind of type is not supported, supported are \
         structs, tuples, arrays, pointers, and references",
    ))
}

impl IsStatic for AngleBracketedGenericArguments {
    fn is_static(&self) -> Result<bool> {
        let lf_static = Lifetime::new("'static", self.span());

        self.args
            .iter()
            .map(|arg| match arg {
                GenericArgument::Lifetime(lf) => Ok(lf == &lf_static),
                GenericArgument::Type(ty) => ty.is_static(),
                _ => variant_type_error(arg.span()),
            })
            .try_fold(true, |is_static, this| {
                this.map(|this_static| is_static && this_static)
            })
    }
}

impl IsStatic for ParenthesizedGenericArguments {
    fn is_static(&self) -> Result<bool> {
        let inputs_static = self
            .inputs
            .iter()
            .map(|ty| ty.is_static())
            .try_fold(true, |is_static, this| {
                this.map(|this_static| is_static && this_static)
            })?;

        let output_static = match &self.output {
            ReturnType::Default => true,
            ReturnType::Type(_, ty) => ty.is_static()?,
        };

        Ok(inputs_static && output_static)
    }
}

impl IsStatic for Type {
    fn is_static(&self) -> Result<bool> {
        use Type::*;

        let lf_static = Lifetime::new("'static", self.span());

        match self {
            Array(arr) => arr.elem.is_static(),
            Paren(p) => p.elem.is_static(),
            Ptr(p) => p.elem.is_static(),

            Reference(r) => {
                if r.lifetime.as_ref().map_or(false, |lf| lf != &lf_static) {
                    return Ok(false);
                }

                r.elem.is_static()
            }

            Path(p) => p
                .path
                .segments
                .iter()
                .map(|sg| match &sg.arguments {
                    PathArguments::AngleBracketed(args) => args.is_static(),
                    PathArguments::Parenthesized(args) => args.is_static(),
                    PathArguments::None => Ok(true),
                })
                .try_fold(true, |is_static, this| {
                    this.map(|this_static| is_static && this_static)
                }),

            _ => variant_type_error(self.span()),
        }
    }
}

pub trait ImplItemExt {
    /// Get a method reference out of an impl item
    fn as_method(&self) -> Result<&ImplItemMethod>;
}

impl ImplItemExt for ImplItem {
    fn as_method(&self) -> Result<&ImplItemMethod> {
        match self {
            ImplItem::Method(m) => Ok(m),
            _ => Err(Error::new(
                self.span(),
                "Handler impl block must only contain methods",
            )),
        }
    }
}

pub trait ImplItemMethodExt {
    /// Is the method declared `async`
    fn is_async(&self) -> bool;

    /// Get the return type
    fn ty_ret(&self) -> Type;

    /// Generate an `ArmPattern` from the method's input type
    fn pat_in(&self, use_try: bool, ty_variant: &Type) -> Result<ArmPattern>;
}

impl ImplItemMethodExt for ImplItemMethod {
    fn is_async(&self) -> bool {
        self.sig.asyncness.is_some()
    }

    fn ty_ret(&self) -> Type {
        // If the method has no return type, say it returns the `()` type
        match &self.sig.output {
            ReturnType::Default => Type::Verbatim(quote!(())),
            ReturnType::Type(_, ty) => *ty.clone(),
        }
    }

    fn pat_in(&self, use_try: bool, ty_variant: &Type) -> Result<ArmPattern> {
        let err = if use_try {
            Error::new(
                self.span(),
                "Handler method needs to take a self reference and a T where T: TryFrom<the variant type>",
            )
        } else {
            Error::new(
                self.span(),
                "Handler method needs to take a self reference and a T where T: From<the variant type>",
            )
        };
        let err = Err(err);

        // Check for explicit annotation (via attribute)
        for attr in &self.attrs {
            if attr.path.is_ident(HANDLE_ATTR) {
                // We're interested in this attr, let's try to parse its content
                // as either _ or a Type
                return if attr.parse_args::<Token![_]>().is_ok() {
                    // This is a catch-all handler
                    // Verify num of input args to catch most obvious usage problems
                    if self.sig.inputs.len() != 2 {
                        let tt = ty_variant.to_token_stream();
                        return Err(Error::new(
                            self.span(),
                            format!("The catch-all method should take two arguments: A self reference and the variant type (ie. `{}`)", tt)
                        ));
                    }

                    Ok(ArmPattern::CatchAll)
                } else {
                    // This will try to parse as Type
                    attr.parse_args().map(ArmPattern::Type)
                };
            }
        }

        let inputs = &self.sig.inputs;
        if inputs.len() != 2 {
            return err;
        }

        // At this point we know there wasn't any explicit annotation,
        // in that case try to extract the variant type out of the method input args.

        // Verify that the first arg is `self`-ish kind of argument
        if let FnArg::Typed(_) = inputs[0] {
            return err;
        }

        // Verify the second argument is a regular (Typed) argument
        let ty = match &inputs[1] {
            FnArg::Typed(pt) => &pt.ty,
            _ => return err,
        };

        Ok(ArmPattern::Type((**ty).clone()))
    }
}
