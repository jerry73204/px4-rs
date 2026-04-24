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

    let topic_items: Vec<TokenStream> = laid
        .topics
        .iter()
        .map(|topic_name| emit_topic(&laid.name, topic_name, size, &laid.constants))
        .collect();

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

        #(#topic_items)*
    }
}

/// Emit a per-topic ZST + `OrbMetadata` static + `impl UorbTopic`.
///
/// PX4 messages can publish under multiple topic names via the
/// `# TOPICS` directive, so a single payload struct gets one of these
/// blocks per topic. The synthesized `orb_metadata` uses the topic
/// name as `o_name`; PX4's broker resolves nodes by name, so this
/// interoperates with C++ publishers/subscribers of the same name.
///
/// Limitations vs. PX4's canonical metadata:
///   * `message_hash` is `0` (compatibility check disabled — use the
///     same struct definition on both sides).
///   * `o_id` is `u16::MAX` (sentinel; not used by the standard
///     orb_advertise/publish/subscribe path).
fn emit_topic(
    struct_name: &str,
    topic_name: &str,
    size: usize,
    constants: &[Constant],
) -> TokenStream {
    let struct_ident = Ident::new(struct_name, Span::call_site());
    let topic_ident = Ident::new(topic_name, Span::call_site());
    let meta_ident = format_ident!("__ORB_META_{}", topic_name.to_uppercase());
    let cname_ident = format_ident!("__ORB_NAME_{}", topic_name.to_uppercase());
    let topic_cstr = format!("{topic_name}\0");
    let size_u16 = size as u16;

    // Pull queue size from a `ORB_QUEUE_LENGTH` constant if present;
    // otherwise default to 1 (PX4's default queue depth).
    let queue: TokenStream = constants
        .iter()
        .find(|c| c.name == "ORB_QUEUE_LENGTH")
        .map(|c| c.value.parse::<TokenStream>().unwrap_or_else(|_| quote!(1)))
        .unwrap_or_else(|| quote!(1));

    quote! {
        #[allow(non_camel_case_types)]
        pub struct #topic_ident;

        #[allow(non_upper_case_globals)]
        const #cname_ident: &'static ::core::ffi::CStr = unsafe {
            ::core::ffi::CStr::from_bytes_with_nul_unchecked(#topic_cstr.as_bytes())
        };

        #[allow(non_upper_case_globals)]
        static #meta_ident: ::px4_uorb::OrbMetadata = ::px4_uorb::OrbMetadata::new(
            ::px4_sys::orb_metadata {
                o_name: #cname_ident.as_ptr(),
                o_size: #size_u16,
                o_size_no_padding: #size_u16,
                message_hash: 0,
                o_id: u16::MAX,
                o_queue: (#queue) as u8,
            }
        );

        impl ::px4_uorb::UorbTopic for #topic_ident {
            type Msg = #struct_ident;
            fn metadata() -> &'static ::px4_sys::orb_metadata {
                #meta_ident.get()
            }
        }
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
