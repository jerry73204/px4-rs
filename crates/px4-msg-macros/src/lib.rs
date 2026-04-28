//! `#[px4_message("path/to/Foo.msg")]` — attribute macro.
//!
//! The attribute is applied to an empty struct item:
//!
//! ```ignore
//! #[px4_message("msg/SensorGyro.msg")]
//! pub struct SensorGyro;
//! ```
//!
//! and replaced with the generated `#[repr(C)] pub struct SensorGyro { ... }`
//! plus constants, topic list, and a compile-time size assertion.
//!
//! Path resolution: the path is taken relative to `CARGO_MANIFEST_DIR`
//! (the crate invoking the macro), or may be absolute. Nested types
//! are resolved against the parent directory of the given path.
//!
//! Crate-path resolution: the emitted `OrbMetadata` / `UorbTopic`
//! impls reference `px4-uorb` and `px4-sys`. We use
//! `proc-macro-crate` to detect whether the user has those direct
//! deps or only the `px4` umbrella, and emit the matching path.

use std::path::PathBuf;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use px4_msg_codegen::EmitPaths;
use quote::quote;
use syn::{parse_macro_input, Ident, LitStr};

#[proc_macro_attribute]
pub fn px4_message(attr: TokenStream, item: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(attr as LitStr);
    // Parse and throw away the stub struct — we emit our own.
    let _stub = parse_macro_input!(item as syn::ItemStruct);

    let raw = path_lit.value();
    let mut path = PathBuf::from(&raw);
    if path.is_relative() {
        if let Ok(root) = std::env::var("CARGO_MANIFEST_DIR") {
            path = PathBuf::from(root).join(path);
        }
    }

    let paths = resolve_paths();
    match px4_msg_codegen::generate_with_paths(&path, Vec::new(), &paths) {
        Ok(ts) => ts.into(),
        Err(e) => syn::Error::new(path_lit.span(), format!("px4_message: {e}"))
            .to_compile_error()
            .into(),
    }
}

/// Emit paths for the `OrbMetadata` / `UorbTopic` blocks. Prefer
/// direct deps (`::px4_uorb`, `::px4_sys`); fall back to the `px4`
/// umbrella (`::px4`, `::px4::sys`) when those aren't present.
fn resolve_paths() -> EmitPaths {
    EmitPaths {
        uorb: resolve("px4-uorb")
            .unwrap_or_else(|| resolve("px4").unwrap_or_else(|| quote!(::px4_uorb))),
        sys: resolve_sys(),
    }
}

fn resolve(name: &str) -> Option<TokenStream2> {
    match crate_name(name).ok()? {
        FoundCrate::Itself => Some(quote!(crate)),
        FoundCrate::Name(n) => {
            let id = Ident::new(&n, Span::call_site());
            Some(quote!(::#id))
        }
    }
}

/// `px4-sys` is reachable directly or through `px4::sys` — the
/// umbrella re-exports the whole crate as a submodule.
fn resolve_sys() -> TokenStream2 {
    if let Some(direct) = resolve("px4-sys") {
        return direct;
    }
    match resolve("px4") {
        Some(px4) => quote!(#px4::sys),
        None => quote!(::px4_sys),
    }
}
