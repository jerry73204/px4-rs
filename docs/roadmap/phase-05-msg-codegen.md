# Phase 05 — `px4-msg-codegen` + `#[px4_message]` macro

**Goal**: Generate `#[repr(C)]` Rust structs from PX4 `msg/*.msg` files
with layouts that byte-match the C++ `px4_msgs::*` types.

**Status**: Not Started
**Priority**: P0 (blocker for px4-uorb)
**Depends on**: Phase 01

## Reference

- PX4 msg parser: `Tools/msg/` (Python, not reusable from Rust directly)
- Abandoned `px4` crate `#[px4_message]` macro — good reference for
  field ordering + padding logic

## Work items

- [ ] 05.1 — `px4-msg-codegen` library crate (std-only, build-time use)
      with a `parse(path: &Path) -> Result<MsgDef>` API
- [ ] 05.2 — Support PX4 msg syntax: scalar primitives, arrays
      (`uint8[8]`), nested types (`sensor_gyro_status`), constants,
      `# TOPICS ...` directive
- [ ] 05.3 — Padding insertion matching PX4's C++ struct layout
      (align to `alignof(T)`; explicit `uint64_t timestamp` always first
      in published topics)
- [ ] 05.4 — Emitter: writes a Rust module with `#[repr(C)]` struct +
      `impl UorbTopic for ...`
- [ ] 05.5 — `px4-msg-macros` proc-macro crate: `#[px4_message("msg/foo.msg")]`
      calls the codegen lib
- [ ] 05.6 — `xtask gen-msgs --px4 <path>` to bulk-generate (for
      consumers that prefer committed code over proc-macros)
- [ ] 05.7 — Layout test harness: for each generated type, emit a
      `static_assertions::assert_eq_size!` against the C++ sizeof from
      `px4_sys` — caught at compile time

## Acceptance criteria

- [ ] `#[px4_message("${PX4}/msg/SensorGyro.msg")] pub struct SensorGyro;`
      compiles and produces a struct with fields matching the C++ header
- [ ] `size_of::<SensorGyro>() == sizeof(sensor_gyro_s)` enforced at
      compile time
- [ ] At least one `# TOPICS` directive (e.g., `actuator_outputs`) is
      handled — emits multiple topic-metadata constants for the same
      struct
- [ ] Works against PX4 main branch and the v1.14 release — no churn

## Out of scope

- Legacy ROS-style `uORB::topics::*.msg` header generation (PX4 does
  that itself)
- Runtime reflection (field names, types at runtime) — adds weight with
  no clear customer
