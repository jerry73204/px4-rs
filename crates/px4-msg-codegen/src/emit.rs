//! Rust-source emitter for a laid-out PX4 message.

use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};

use crate::layout::{LaidOutField, LaidOutMsg, rust_type_for};
use crate::model::Constant;

/// Emit a `#[repr(C)] pub struct <Name>` + constants + topic list +
/// a compile-time size assertion.
pub fn emit(laid: &LaidOutMsg) -> TokenStream {
    let name = Ident::new(&laid.name, Span::call_site());

    let fields: Vec<TokenStream> = laid
        .fields
        .iter()
        .map(|f| match f {
            LaidOutField::Real(field) => {
                let fname = sanitize_ident(&field.name);
                let ty_str = rust_type_for(&field.ty);
                let ty: TokenStream = ty_str.parse().expect("valid rust type string");
                quote!(pub #fname: #ty)
            }
            LaidOutField::Padding { index, size } => {
                let pname = format_ident!("_padding{}", index);
                quote!(pub #pname: [u8; #size])
            }
        })
        .collect();

    let consts = laid.constants.iter().map(emit_constant);

    let topic_lits: Vec<TokenStream> = laid.topics.iter().map(|t| quote!(#t)).collect();
    let n_topics = topic_lits.len();

    let size = laid.size;

    quote! {
        #[repr(C)]
        #[derive(Copy, Clone)]
        pub struct #name {
            #(#fields),*
        }

        impl #name {
            #(#consts)*
            pub const TOPICS: [&'static str; #n_topics] = [ #(#topic_lits),* ];
        }

        const _: () = assert!(
            ::core::mem::size_of::<#name>() == #size,
            concat!("px4-msg-codegen: layout size mismatch for ", stringify!(#name)),
        );
    }
}

fn emit_constant(c: &Constant) -> TokenStream {
    let name = Ident::new(&c.name, Span::call_site());
    let ty_str = c.ty.rust_type();
    let ty: TokenStream = ty_str.parse().unwrap();
    let value: TokenStream = c.value.parse().unwrap_or_else(|_| {
        panic!(
            "px4-msg-codegen: cannot tokenize constant value `{}`",
            c.value
        )
    });
    quote! {
        pub const #name: #ty = #value;
    }
}

/// PX4 field names are lowercase snake_case and fit Rust ident rules
/// unmodified, but a field literally named `type`, `match`, etc. would
/// collide with Rust keywords. Prefix with `r#` when that happens.
fn sanitize_ident(name: &str) -> Ident {
    if is_rust_keyword(name) {
        Ident::new_raw(name, Span::call_site())
    } else {
        Ident::new(name, Span::call_site())
    }
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
    )
}
