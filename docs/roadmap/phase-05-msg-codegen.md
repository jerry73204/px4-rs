# Phase 05 — `px4-msg-codegen` + `#[px4_message]` macro

**Goal**: Generate `#[repr(C)]` Rust structs from PX4 `msg/*.msg` files
with layouts that byte-match the C++ `px4_msgs::*` types.

**Status**: Complete
**Priority**: P0 (blocker for px4-uorb)
**Depends on**: Phase 01

## Reference

- PX4 msg parser: `Tools/msg/` (Python, not reusable from Rust directly)
- Abandoned `px4` crate `#[px4_message]` macro — good reference for
  field ordering + padding logic

## Work items

- [x] 05.1 — `crates/px4-msg-codegen/` library with `parse_file`,
      `Resolver::layout`, and `emit` (plus a one-shot `generate()`).
- [x] 05.2 — Parser supports scalars (bool/char/int8..int64/uint8..uint64/
      float32/float64), `T[N]` arrays, nested `CamelCase` types,
      `T NAME = VALUE` constants, `# TOPICS` directive, trailing `#`
      comments.
- [x] 05.3 — Layout engine replicates PX4's Python logic
      (`Tools/msg/px_generate_uorb_topic_helper.py::add_padding_bytes`):
      stable sort by size descending, pad before each nested type to
      8-byte alignment, pad tail to 8-byte alignment.
- [x] 05.4 — Emitter produces `#[repr(C)] #[derive(Copy, Clone)] pub
      struct <Name>` plus an `impl` block with user constants and a
      `TOPICS: [&'static str; N]` array. The `UorbTopic` trait impl is
      deferred to phase 06 (it lives in `px4-uorb`, where it will be
      regenerated alongside the `orb_metadata` statics).
- [x] 05.5 — `crates/px4-msg-macros/` with `#[px4_message("path")]`
      attribute — reads the file at compile time, resolves nested types
      against its parent directory.
- [x] 05.6 — `xtask gen-msgs [--px4 DIR] [--out DIR]` bulk-generates
      one `.rs` file per `.msg`. 198/198 PX4 v1.16.2 messages succeed.
      Output is gitignored under `crates/px4-msg-codegen/generated/`.
- [x] 05.7 — Compile-time layout assertion baked into each emitted
      struct via `const _: () = assert!(size_of::<T>() == N)`. No
      `static_assertions` dep needed.

## Acceptance criteria

- [x] `#[px4_message("tests/fixtures/SensorGyro.msg")] pub struct SensorGyro;`
      compiles and produces a struct with fields matching the C++ header
      (see `crates/px4-msg-macros/tests/macro_expand.rs`)
- [x] `size_of::<SensorGyro>() == 48` enforced at compile time via the
      generated `const _: () = assert!(...)`
- [x] `# TOPICS actuator_outputs actuator_outputs_sim actuator_outputs_debug`
      is handled — each entry lands in the struct's `TOPICS` constant
      (covered by `crates/px4-msg-codegen/src/layout.rs::tests::actuator_outputs_with_topics`)
- [x] Works against PX4 v1.16.2: 198/198 messages in the tree
      round-trip (parser + layout) with zero skips
      (`crates/px4-msg-codegen/tests/real_tree.rs`)

## Out of scope

- Legacy ROS-style `uORB::topics::*.msg` header generation (PX4 does
  that itself)
- Runtime reflection (field names, types at runtime) — adds weight with
  no clear customer
