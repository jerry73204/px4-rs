//! Raw FFI bindings to PX4 Autopilot.
//!
//! This crate is the only place where `unsafe extern "C"` declarations to
//! PX4 live. Higher-level typed wrappers are in `px4-workqueue`,
//! `px4-uorb`, and `px4-log`.
//!
//! # Minimum supported PX4 version
//!
//! **v1.15.0.** The `orb_metadata` struct layout is incompatible on
//! v1.14 and earlier (see `docs/roadmap/phase-02-px4-sys.md`). When
//! `PX4_AUTOPILOT_DIR` is set, `build.rs` refuses to build against a
//! pre-v1.15 tree.

#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// When the C++ trampolines weren't compiled (host builds without
// PX4_RS_BUILD_TRAMPOLINES), provide a Rust-side stub so downstream
// crates that call `px4_rs_find_orb_meta` still link. The stub always
// returns null, and the caller falls back to the codegen-synthesized
// `orb_metadata`. The Rust item name is prefixed to avoid colliding
// with the bindgen-generated extern declaration; `#[export_name]`
// publishes it under the FFI symbol the caller expects.
#[cfg(not(px4_rs_trampolines))]
#[unsafe(export_name = "px4_rs_find_orb_meta")]
unsafe extern "C" fn __px4_rs_stub_find_orb_meta(
    _name: *const ::core::ffi::c_char,
) -> *const orb_metadata {
    ::core::ptr::null()
}
