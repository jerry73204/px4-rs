//! `#[px4::main]` — entry-point attribute macro.
//!
//! Wraps a Rusty `fn main(args: Args) -> Result<…, …>` in the C
//! `<name>_main(int argc, char *argv[])` shape PX4's shell expects.
//!
//! Expansion of
//!
//! ```ignore
//! #[px4::main]
//! fn main(args: px4::Args) -> Result<(), &'static str> { /* … */ }
//! ```
//!
//! is roughly:
//!
//! ```ignore
//! const MODULE_NAME: &::core::ffi::CStr = c"<name>";
//!
//! fn main(args: px4::Args) -> Result<(), &'static str> { /* … */ }
//!
//! #[unsafe(no_mangle)]
//! pub extern "C" fn <name>_main(
//!     argc: ::core::ffi::c_int,
//!     argv: *mut *mut ::core::ffi::c_char,
//! ) -> ::core::ffi::c_int {
//!     let __args = unsafe { ::px4::Args::from_raw(argc, argv) };
//!     let __r = main(__args);
//!     ::px4::ModuleResult::into_c_int(__r, MODULE_NAME)
//! }
//! ```
//!
//! `<name>` defaults to `CARGO_PKG_NAME` (with `-` translated to
//! `_`); override with `#[px4::main(name = "...")]`.
//!
//! The macro emits `MODULE_NAME` at the call site so the logging
//! macros (`info!`, `err!`, …) can find it. A separate `module!()`
//! call is unnecessary — and combining the two produces a
//! duplicate-const error, which is the loudest possible signal.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, FnArg, Ident, ItemFn, LitStr, ReturnType, Token};

#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as Args);
    let input = parse_macro_input!(item as ItemFn);

    match expand(args, input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Attribute arguments: optional `name = "..."`.
struct Args {
    name: Option<LitStr>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { name: None });
        }
        let pairs: Punctuated<NamedArg, Token![,]> = Punctuated::parse_terminated(input)?;
        let mut name: Option<LitStr> = None;
        for p in pairs {
            match p.key.to_string().as_str() {
                "name" => name = Some(p.value),
                other => {
                    return Err(syn::Error::new_spanned(
                        p.key,
                        format!("unknown #[px4::main] argument `{other}` (expected `name`)"),
                    ));
                }
            }
        }
        Ok(Self { name })
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

fn expand(args: Args, input: ItemFn) -> syn::Result<TokenStream2> {
    // ---- Validate the user's signature -------------------------------
    if let Some(asy) = &input.sig.asyncness {
        return Err(syn::Error::new_spanned(
            asy,
            "#[px4::main] cannot annotate `async fn` — PX4's entry point is synchronous",
        ));
    }
    if input.sig.inputs.len() > 1 {
        return Err(syn::Error::new_spanned(
            &input.sig.inputs,
            "#[px4::main] fn takes at most one argument (an `Args`)",
        ));
    }
    if let Some(arg) = input
        .sig
        .inputs
        .iter()
        .find(|a| matches!(a, FnArg::Receiver(_)))
    {
        return Err(syn::Error::new_spanned(
            arg,
            "#[px4::main] fn cannot take `self`",
        ));
    }

    let user_fn_ident = input.sig.ident.clone();
    let takes_args = !input.sig.inputs.is_empty();

    // ---- Resolve the module name ------------------------------------
    let name_str = match args.name {
        Some(lit) => lit.value(),
        None => std::env::var("CARGO_PKG_NAME")
            .map(|s| s.replace('-', "_"))
            .map_err(|_| {
                syn::Error::new(
                    Span::call_site(),
                    "#[px4::main]: CARGO_PKG_NAME is unset and no `name = \"...\"` was given",
                )
            })?,
    };
    let entry_ident = format_ident!("{}_main", name_str);
    let module_name_lit = format!("{name_str}\0");

    // ---- Build the call --------------------------------------------
    let make_args = if takes_args {
        quote! {
            // SAFETY: PX4's pxh dispatcher passes a well-formed
            // (argc, argv) per platforms/posix/.../pxh.cpp:104-111.
            let __args = unsafe { ::px4::Args::from_raw(argc, argv) };
        }
    } else {
        quote! {}
    };
    let call_user = if takes_args {
        quote! { #user_fn_ident(__args) }
    } else {
        quote! { #user_fn_ident() }
    };

    // ---- Tail signal warnings ---------------------------------------
    // Reject `-> impl Trait` returns that the trait dispatch can't
    // resolve — the trait bound at the call site catches the rest, but
    // surfacing a prettier message here helps.
    if let ReturnType::Type(_, ty) = &input.sig.output {
        if let syn::Type::ImplTrait(_) = &**ty {
            return Err(syn::Error::new_spanned(
                ty,
                "#[px4::main] return type cannot be `impl Trait` — return a \
                 concrete type that implements `px4::ModuleResult` (e.g. \
                 `()`, `c_int`, or `Result<…, E: Display>`)",
            ));
        }
    }

    let expanded = quote! {
        // The user's function — preserved verbatim.
        #input

        // Provide MODULE_NAME for the logging macros at this scope.
        // Subsumes a separate `module!()` call.
        #[allow(dead_code)]
        const MODULE_NAME: &::core::ffi::CStr = unsafe {
            ::core::ffi::CStr::from_bytes_with_nul_unchecked(
                #module_name_lit.as_bytes(),
            )
        };

        #[unsafe(no_mangle)]
        pub extern "C" fn #entry_ident(
            argc: ::core::ffi::c_int,
            argv: *mut *mut ::core::ffi::c_char,
        ) -> ::core::ffi::c_int {
            #make_args
            let __r = #call_user;
            ::px4::ModuleResult::into_c_int(__r, MODULE_NAME)
        }
    };

    Ok(expanded)
}
