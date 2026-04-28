//! `#[task(wq = "...")]` — attribute macro that turns an `async fn` into
//! a statically-allocated PX4 WorkItem task.
//!
//! Expansion of
//!
//! ```ignore
//! #[task(wq = "rate_ctrl")]
//! async fn rate_watch(x: u32) { /* body */ }
//! ```
//!
//! is a module named after the function:
//!
//! ```ignore
//! mod rate_watch {
//!     use super::*;
//!     type __Fut = impl ::core::future::Future<Output = ()>;
//!     static __CELL: #wq::WorkItemCell<__Fut> =
//!         #wq::WorkItemCell::new();
//!     #[::core::prelude::v1::define_opaque(__Fut)]
//!     fn __make(x: u32) -> __Fut {
//!         async move { /* body */ }
//!     }
//!     pub fn spawn(x: u32)
//!         -> Result<#wq::SpawnToken, #wq::SpawnError>
//!     {
//!         __CELL.try_spawn(
//!             __make(x),
//!             &#wq::wq_configurations::rate_ctrl,
//!             /* c-string name */,
//!         )
//!     }
//! }
//! ```
//!
//! The user's crate must enable `#![feature(type_alias_impl_trait)]`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Ident, ItemFn, LitStr, ReturnType, Token};

/// Resolve the path the user's crate uses to reach `px4-workqueue` —
/// either directly (`#wq`) or through the `px4`
/// umbrella (`::px4`). Falls back to the direct name when neither
/// is found in the manifest, which leaves the existing error
/// messaging in place.
fn workqueue_path() -> TokenStream2 {
    if let Ok(found) = crate_name("px4-workqueue") {
        return match found {
            FoundCrate::Itself => quote!(crate),
            FoundCrate::Name(name) => {
                let id = Ident::new(&name, Span::call_site());
                quote!(::#id)
            }
        };
    }
    if let Ok(found) = crate_name("px4") {
        return match found {
            FoundCrate::Itself => quote!(crate),
            FoundCrate::Name(name) => {
                let id = Ident::new(&name, Span::call_site());
                quote!(::#id)
            }
        };
    }
    quote!(::px4_workqueue)
}

/// Attribute arguments: currently just `wq = "name"`.
struct Args {
    wq: LitStr,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        // key = "value" pairs, comma-separated.
        let pairs: Punctuated<NamedArg, Token![,]> = Punctuated::parse_terminated(input)?;
        let mut wq: Option<LitStr> = None;
        for p in pairs {
            match p.key.to_string().as_str() {
                "wq" => wq = Some(p.value),
                other => {
                    return Err(syn::Error::new_spanned(
                        p.key,
                        format!("unknown #[task] argument `{other}` (expected `wq`)"),
                    ));
                }
            }
        }
        let wq = wq.ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "missing required argument: #[task(wq = \"...\")]",
            )
        })?;
        Ok(Self { wq })
    }
}

struct NamedArg {
    key: Ident,
    value: LitStr,
}

impl Parse for NamedArg {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let value: LitStr = input.parse()?;
        Ok(Self { key, value })
    }
}

#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as Args);
    let input = parse_macro_input!(item as ItemFn);

    match expand(args, input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(args: Args, f: ItemFn) -> syn::Result<TokenStream2> {
    if f.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            f.sig.fn_token,
            "#[task] requires an `async fn`",
        ));
    }
    // Reject return types other than `()` or `-> !` to keep the story simple.
    match &f.sig.output {
        ReturnType::Default => {}
        ReturnType::Type(_, ty) => {
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            if ty_str != "()" && ty_str != "!" {
                return Err(syn::Error::new_spanned(
                    ty,
                    "#[task] async fn must return () or !",
                ));
            }
        }
    }

    let fn_name = f.sig.ident.clone();
    let mod_name = fn_name.clone();
    let vis = f.vis.clone();
    let wq_lit = args.wq.value();
    let wq_ident = Ident::new(&wq_lit, args.wq.span());

    // Rebuild the user's function body as `__body` inside the module,
    // keeping its visibility private. Remove `async` from the signature
    // we embed — `__make` will `async move { __body(...).await }` it.
    let body = &f.block;
    let fn_args = &f.sig.inputs;

    // Extract argument identifiers for forwarding.
    let arg_idents: Vec<_> = fn_args
        .iter()
        .map(|a| match a {
            syn::FnArg::Typed(p) => Ok(&p.pat),
            syn::FnArg::Receiver(_) => {
                Err(syn::Error::new_spanned(a, "#[task] fns cannot take `self`"))
            }
        })
        .collect::<Result<_, _>>()?;

    // Leak the task name to the byte string: concat!("fn_name", "\0").
    let name_lit = LitStr::new(&fn_name.to_string(), fn_name.span());

    let wq = workqueue_path();
    let expanded = quote! {
        #[allow(non_snake_case)]
        #vis mod #mod_name {
            use super::*;

            pub(super) type __Fut = impl ::core::future::Future<Output = ()>;

            #[allow(non_upper_case_globals)]
            static __CELL: #wq::WorkItemCell<__Fut> =
                #wq::WorkItemCell::new();

            async fn __body(#fn_args) #body

            #[::core::prelude::v1::define_opaque(__Fut)]
            fn __make(#fn_args) -> __Fut {
                async move { __body(#(#arg_idents),*).await }
            }

            /// Spawn this task on its configured work queue. Returns
            /// `Err(SpawnError::Busy)` if the task is already running.
            pub fn try_spawn(
                #fn_args
            ) -> ::core::result::Result<
                #wq::SpawnToken,
                #wq::SpawnError,
            > {
                const __NAME: &::core::ffi::CStr = unsafe {
                    ::core::ffi::CStr::from_bytes_with_nul_unchecked(
                        ::core::concat!(#name_lit, "\0").as_bytes(),
                    )
                };
                __CELL.try_spawn(
                    __make(#(#arg_idents),*),
                    &#wq::wq_configurations::#wq_ident,
                    __NAME,
                )
            }

            /// Spawn this task and panic on `Busy`. Use for cold-start
            /// code where a second spawn is a programmer error.
            pub fn spawn(#fn_args) -> #wq::SpawnToken {
                const __NAME: &::core::ffi::CStr = unsafe {
                    ::core::ffi::CStr::from_bytes_with_nul_unchecked(
                        ::core::concat!(#name_lit, "\0").as_bytes(),
                    )
                };
                __CELL.spawn(
                    __make(#(#arg_idents),*),
                    &#wq::wq_configurations::#wq_ident,
                    __NAME,
                )
            }
        }
    };

    Ok(expanded)
}
