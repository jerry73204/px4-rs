# Phase 02 — `px4-sys` FFI bindings

**Goal**: Expose a minimal, audited set of PX4 C/C++ symbols to Rust.

**Status**: Not Started
**Priority**: P0
**Depends on**: Phase 01

## Scope

Only the FFI surface that later phases need:

- `px4_platform_common/px4_work_queue/WorkItem.h` — `ScheduleNow`, ctor/dtor
- `px4_platform_common/px4_work_queue/WorkQueueManager.hpp` — `Attach`, `Detach`, `WorkQueueFindOrCreate`
- `drivers/drv_hrt.h` — `hrt_absolute_time`, `hrt_call_every`, `hrt_cancel`
- `uORB/uORB.h` + `uORB/topics/*.h` — `orb_advertise_multi`, `orb_publish`,
  `orb_subscribe`, `orb_copy`, `orb_check`, `orb_register_callback`,
  `orb_unregister_callback`, `orb_metadata`
- `px4_platform_common/log.h` — `px4_log_modulename`, `_PX4_LOG_LEVEL_INFO`, ...

## Work items

- [ ] 02.1 — Add `crates/px4-sys/` with `links = "px4"` and a `build.rs`
      using `bindgen` 0.70
- [ ] 02.2 — `wrapper.h` that includes only the headers above (no pulling
      in all of PX4)
- [ ] 02.3 — Vendor a generated `bindings.rs` snapshot for host-doc builds
      (when `PX4_AUTOPILOT_DIR` is unset, `build.rs` uses the snapshot)
- [ ] 02.4 — C++ linkage: uORB and WorkQueue symbols are `extern "C++"`.
      Emit `extern "C"` shim `.cpp` trampolines compiled by `cc` crate.
- [ ] 02.5 — `just gen-sys` xtask command that regenerates bindings and
      writes the snapshot back into the tree
- [ ] 02.6 — `#![no_std]` on `px4-sys` (only `-sys` declarations, no impls)
- [ ] 02.7 — Feature-gate per-family `orb_metadata` blocks so host builds
      don't need PX4 msg headers

## Acceptance criteria

- [ ] `cargo build -p px4-sys` succeeds on the host against the vendored
      snapshot (no PX4 checkout needed)
- [ ] `cargo build -p px4-sys --target thumbv7em-none-eabihf` succeeds
      when `PX4_AUTOPILOT_DIR` points at a checkout
- [ ] The generated `bindings.rs` contains `ScheduleNow`, `orb_publish`,
      and `hrt_call_every` by name
- [ ] No `extern "C++"` ABI crossings in user-facing code — only through
      `.cpp` trampolines compiled by this crate

## Risks

- PX4's `WorkQueue` ctor is `delete`-d and items must be constructed by
  subclassing in C++. Likely need a `px4_rust_work_item.cpp` trampoline
  that exposes a C-callable `make/destroy/run` interface.
- uORB `orb_register_callback` takes a `uORB::SubscriptionCallback *`
  (C++). The trampoline pattern above will wrap this too.

## Out of scope

- High-level typed wrappers (those live in `px4-workqueue` / `px4-uorb`)
- Any `extern "Rust"` glue
- Parameter API (ParamGet/Set) — deferred to a later phase
