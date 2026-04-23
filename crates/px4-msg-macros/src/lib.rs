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

use std::path::PathBuf;

use proc_macro::TokenStream;
use syn::{parse_macro_input, LitStr};

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

    match px4_msg_codegen::generate(&path, Vec::new()) {
        Ok(ts) => ts.into(),
        Err(e) => syn::Error::new(path_lit.span(), format!("px4_message: {e}"))
            .to_compile_error()
            .into(),
    }
}
