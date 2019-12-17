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

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{Error, GenericParam, Ident, ImplItemMethod, ItemImpl, Result, Signature, Token, Type};

use crate::extensions::*;
use crate::unvariant::ArmPattern;

type GenericsInner = Punctuated<GenericParam, Token![,]>;

/// #[handler(...)] macro arguments and associated data
///
/// Syntax of sync handler:
/// ```ignore
/// #[handler([try] <VariantType> [trait <CustomHandlerTrait>])]
/// ```
///
/// Syntax of async handler:
/// ```ignore
/// #[handler(async [try] <VariantType> [suffix <fn_name_suffix>])]
/// ```
///
struct Args {
    ty: Type,
    ty_static: bool,
    use_async: bool,
    use_try: bool,
    handler_suffix: Option<Ident>,
    handler_trait: Option<Type>,
}

mod keyword {
    syn::custom_keyword!(suffix);
}

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        let use_async = input.peek(Token![async]);
        if use_async {
            input.parse::<Token![async]>()?;
        }

        let use_try = input.peek(Token![try]);
        if use_try {
            input.parse::<Token![try]>()?;
        }

        let ty: Type = input.parse()?;
        let ty_static = ty.is_static()?;

        let have_suffix = use_async && input.peek(keyword::suffix);
        let handler_suffix = if have_suffix {
            input.parse::<keyword::suffix>()?;
            Some(input.parse::<Ident>()?)
        } else {
            None
        };

        let handler_trait = input.peek(Token![trait]);
        let handler_trait = if handler_trait {
            let token = input.parse::<Token![trait]>()?;
            if use_async {
                return Err(Error::new(
                    token.span(),
                    "The trait argument is not supported in async handler",
                ));
            }

            Some(input.parse::<Type>()?)
        } else {
            None
        };

        // `@` separates the macro args from the impl item, fetch & discard it here
        input.parse::<Token![@]>().map_err(|err| {
            Error::new(
                err.span(),
                "Unexpected tokens, expected end of handler(...) arguments",
            )
        })?;

        Ok(Self {
            ty,
            ty_static,
            use_async,
            use_try,
            handler_trait,
            handler_suffix,
        })
    }
}

/// Holds data about the catch-all method
/// which is called when an unknown ID is extracted
/// or when the `try` handler is being used and there
/// was a conversion error.
struct CatchAllHandler {
    lifetimes: GenericsInner,
    method: Ident,
    await_suffix: Option<TokenStream>,
}

impl CatchAllHandler {
    fn new(sig: &Signature, is_async: bool) -> Self {
        let await_suffix = if is_async { Some(quote!(.await)) } else { None };

        CatchAllHandler {
            lifetimes: sig.generics.params.clone(),
            method: sig.ident.clone(),
            await_suffix,
        }
    }

    fn to_tokens(&self, tokens: &mut TokenStream, item: &Item) {
        let method = &self.method;
        let await_suffix = &self.await_suffix;

        let arm = if item.args.use_try {
            quote!(_ => self.#method(Ok(variant))#await_suffix,)
        } else {
            quote!(_ => self.#method(variant)#await_suffix,)
        };

        tokens.extend(arm);
    }
}

/// Holds data about handler methods that are
/// needed for the match statement generation.
struct Handler {
    ty: Type,
    method: Ident,
    await_suffix: Option<TokenStream>,
}

impl Handler {
    fn new(ty: Type, sig: &Signature, is_async: bool) -> Self {
        let await_suffix = if is_async { Some(quote!(.await)) } else { None };

        Self {
            ty,
            method: sig.ident.clone(),
            await_suffix,
        }
    }

    fn to_tokens(&self, tokens: &mut TokenStream, item: &Item) {
        let method = &self.method;
        let ty = &self.ty;
        let catch_all = &item.catch_all.method;

        let await_suffix = &self.await_suffix;
        let catch_all_suffix = &item.catch_all.await_suffix;

        // The Into/TryInto conversion style leads to the best error message,
        // or at least that's what I've seen so far...
        let arm = if item.args.use_try {
            quote!(<#ty as ::ii_unvariant::Id<_>>::ID => {
                match ::std::convert::TryInto::<_>::try_into(variant) {
                    Ok(data) => self.#method(data)#await_suffix,
                    Err(err) => self.#catch_all(Err(err.into()))#catch_all_suffix,
                }
            })
        } else {
            quote!(<#ty as ::ii_unvariant::Id<_>>::ID => {
                let data = ::std::convert::Into::<_>::into(variant);
                self.#method(data)#await_suffix
            })
        };

        tokens.extend(arm);
    }
}

/// A `VisitMut` visitor that removes `#[handle(..)]` attributes
/// from `impl` methods signatures. Once we've parsed the type out of
/// the `#[handle(..)]` attrib, we want it removed - it's just an annotation,
/// not a real attribute known to the compiler.
struct HandleAttrRemover;

impl VisitMut for HandleAttrRemover {
    fn visit_impl_item_method_mut(&mut self, method: &mut ImplItemMethod) {
        method.attrs.retain(|attr| !attr.path.is_ident(HANDLE_ATTR));
    }
}

/// Holds the entire `impl` block, macro arguments (`Args`) as well as
/// some associated metadata parsed from within the impl block, such
/// as the return type and handlers metadata.
pub struct Item {
    args: Args,
    iimpl: ItemImpl,
    ty_ret: Type,
    handlers: Vec<Handler>,
    catch_all: CatchAllHandler,
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self> {
        let args: Args = input.parse()?;
        let ty_variant = &args.ty;
        let iimpl: ItemImpl = input.parse()?;

        if args.use_async && !iimpl.generics.params.is_empty() {
            return Err(Error::new(
                iimpl.generics.params.span(),
                "Generic async handlers are not supported",
            ));
        } else if let Some(where_clause) = iimpl.generics.where_clause.as_ref() {
            return Err(Error::new(
                where_clause.span(),
                "A where clause is not supported in handler implementation",
            ));
        }

        // Get the first method
        let first = match iimpl.items.get(0) {
            Some(i) => i.as_method()?,
            None => return Err(Error::new(iimpl.span(), "Empty handler impl block")),
        };

        // Get first method metadata
        let first_is_async = args.use_async && first.is_async();
        let ty_ret = first.ty_ret();
        let pat_in = first.pat_in(args.use_try, &ty_variant)?;

        let mut handlers = vec![];

        let mut catch_all = match pat_in {
            ArmPattern::Type(ty) => {
                handlers.push(Handler::new(ty, &first.sig, first_is_async));
                None
            }
            ArmPattern::CatchAll => Some(CatchAllHandler::new(&first.sig, first_is_async)),
        };

        // Iterate the other methods
        for item in &iimpl.items[1..] {
            let method = item.as_method()?;
            let is_async = args.use_async && method.is_async();

            // Check return type
            let method_ret = method.ty_ret();
            if method_ret != ty_ret {
                return Err(Error::new(
                    method_ret.span(),
                    "All handler methods must return the same type",
                ));
            }

            // Get input type
            let method_in = method.pat_in(args.use_try, &ty_variant)?;

            match method_in {
                ArmPattern::Type(ty) => {
                    handlers.push(Handler::new(ty, &method.sig, is_async));
                }
                ArmPattern::CatchAll => {
                    if catch_all.is_some() {
                        return Err(Error::new(
                            method.span(),
                            "Multiple catch-all handlers found",
                        ));
                    }

                    catch_all = Some(CatchAllHandler::new(&method.sig, is_async));
                }
            }
        }

        if let Some(catch_all) = catch_all {
            Ok(Self {
                args,
                iimpl,
                ty_ret,
                handlers,
                catch_all,
            })
        } else {
            Err(Error::new(
                iimpl.span(),
                "No catch-all handler found: Please add a method annotated with #[handle(_)]",
            ))
        }
    }
}

impl Item {
    fn handler_impl(&self, handlers: TokenStream) -> TokenStream {
        let ty_self = &self.iimpl.self_ty;
        let ty_ret = &self.ty_ret;
        let ty_var = &self.args.ty;
        let ty_var_lifetimes = &self.catch_all.lifetimes;

        let handler_trait = if let Some(handler_trait) = self.args.handler_trait.as_ref() {
            quote!(#handler_trait)
        } else {
            quote!(::ii_unvariant::Handler<#ty_var, #ty_ret>)
        };

        let (impl_generics, _, where_clause) = self.iimpl.generics.split_for_impl();

        quote!(
            impl #impl_generics #handler_trait for #ty_self #where_clause {
                fn handle<#ty_var_lifetimes>(&mut self, variant: #ty_var) -> #ty_ret {
                    match ::ii_unvariant::GetId::get_id(&variant) {
                        #handlers
                    }
                }
            }
        )
    }

    fn async_handler_impl(&self, handlers: TokenStream) -> TokenStream {
        let ty_self = &self.iimpl.self_ty;
        let ty_var = &self.args.ty;
        let ty_ret = &self.ty_ret;
        let ty_var_lifetimes = &self.catch_all.lifetimes;

        let handler_suffix = &self.args.handler_suffix.as_ref();

        let fn_handle_async = handler_suffix
            .map(|suffix| Ident::new(&format!("handle{}", suffix), Span::call_site()))
            .unwrap_or_else(|| Ident::new("handle", Span::call_site()));

        let fn_into_handler = handler_suffix
            .map(|suffix| Ident::new(&format!("into_handler{}", suffix), Span::call_site()))
            .unwrap_or_else(|| Ident::new("into_handler", Span::call_site()));

        let mut res = quote!(
            impl #ty_self {
                pub async fn #fn_handle_async<#ty_var_lifetimes>(&mut self, variant: #ty_var) -> #ty_ret {
                    match ::ii_unvariant::GetId::get_id(&variant) {
                        #handlers
                    }
                }
            }
        );

        let handler = quote! {
            impl #ty_self {
                pub fn #fn_into_handler(self) -> ::ii_unvariant::AsyncHandler<#ty_var, #ty_ret> {
                    use ::std::future::Future;
                    use ::ii_unvariant::{AsyncHandler, HandlerFilter};

                    fn handle_fn(
                        handler: *mut #ty_self,
                        variant: #ty_var,
                    ) -> impl Future<Output = #ty_ret> {
                        let handler = unsafe { &mut *handler };
                        let variant = variant;
                        handler.#fn_handle_async(variant)
                    }

                    let filter = HandlerFilter::__new(self, handle_fn);
                    AsyncHandler::__new(filter)
                }
            }
        };

        if self.args.ty_static {
            res.extend(handler);
        }

        res
    }

    fn remove_handle_attrs(&mut self) {
        HandleAttrRemover.visit_item_impl_mut(&mut self.iimpl);
    }
}

impl ToTokens for Item {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let iimpl = &self.iimpl;

        let mut handlers = TokenStream::new();
        for handler in &self.handlers {
            handler.to_tokens(&mut handlers, &self);
        }

        self.catch_all.to_tokens(&mut handlers, &self);

        let handler_impl = if self.args.use_async {
            self.async_handler_impl(handlers)
        } else {
            self.handler_impl(handlers)
        };

        tokens.extend(quote!(
            #iimpl
            #handler_impl
        ));
    }
}

pub fn expand(mut item: Item) -> TokenStream {
    item.remove_handle_attrs();
    quote!(#item)
}
