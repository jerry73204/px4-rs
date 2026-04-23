//! Parser + layout + Rust emitter for PX4 uORB `.msg` files.
//!
//! ```text
//!   parse_file / parse_str   →   MsgDef (raw)
//!   Resolver::layout         →   LaidOutMsg (sorted + padded, with size)
//!   emit::emit               →   Rust TokenStream (#[repr(C)] struct + consts)
//! ```
//!
//! This crate is `std`-only and intended for use at build time — from
//! `xtask gen-msgs` or from the `px4-msg-macros` proc-macro. The
//! generated code itself is `no_std`.

pub mod emit;
pub mod layout;
pub mod model;
pub mod parser;

pub use emit::emit;
pub use layout::{LaidOutField, LaidOutMsg, Resolver};
pub use model::{Constant, Field, FieldType, MsgDef, ParseError, Scalar};
pub use parser::{parse_file, parse_str};

use std::path::Path;

/// One-shot: parse, lay out, emit a Rust module for a single `.msg`
/// file. `search_path` is used to resolve nested types (defaults to
/// the parent directory of `path` if empty).
pub fn generate(
    path: &Path,
    search_path: Vec<std::path::PathBuf>,
) -> Result<proc_macro2::TokenStream, ParseError> {
    let def = parse_file(path)?;

    let mut search = search_path;
    if search.is_empty() {
        if let Some(parent) = path.parent() {
            search.push(parent.to_path_buf());
        }
    }

    let mut resolver = Resolver::new(search);
    let laid = resolver.layout(&def)?;
    Ok(emit(&laid))
}
