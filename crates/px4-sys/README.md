# `px4-sys`

Raw `unsafe extern "C"` bindings to PX4 Autopilot.

This crate is the FFI floor of [`px4-rs`](../../README.md). Everything
here is `#[no_std]` and audit-only — higher-level, safe Rust lives in
`px4-log`, `px4-workqueue`, and `px4-uorb`.

## Supported PX4 versions

- **Minimum supported**: PX4 v1.15.0.
- **Pinned snapshot / CI**: PX4 v1.16.2.

PX4 v1.14 and earlier use a different `orb_metadata` struct layout. The
`build.rs` checks for the v1.15+ marker (`message_hash`) when
`PX4_AUTOPILOT_DIR` is set, and refuses to build otherwise.

## Build-time environment

| Variable | Purpose |
| --- | --- |
| `PX4_AUTOPILOT_DIR` | Path to a PX4 source tree. If set, `wrapper.cpp` is compiled against it and the version is sanity-checked. If unset, only the Rust bindings are generated (fine for host unit tests and docs.rs). |

For target builds driven by PX4's CMake, `PX4_AUTOPILOT_DIR` is always
set by the `px4_rust_module()` CMake helper.

## Vendored snapshot

A pre-generated `bindings/bindings.rs` is committed to the tree so that
downstream builds work without `libclang`. Regenerate with:

```sh
just gen-sys
```
