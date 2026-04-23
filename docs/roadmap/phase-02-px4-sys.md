# Phase 02 — `px4-sys` FFI bindings

**Goal**: Expose a minimal, audited set of PX4 C/C++ symbols to Rust.

**Status**: Complete
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

## PX4 source: `PX4_AUTOPILOT_DIR`, not submodule

px4-rs does **not** vendor PX4 as a submodule. Consumers set
`PX4_AUTOPILOT_DIR` to point at the same PX4 checkout their firmware is
built from, and `build.rs` generates bindings against *that* tree. Host
and docs.rs builds use the vendored `bindings.rs` snapshot when the env
var is unset.

Why not a submodule: every px4-rs user already has a PX4 tree (that's
where `make px4_fmu-v6x_default` runs). A submodule would give them a
second copy, and if the two drift, the bindings wouldn't match the
headers their firmware actually links against. This isn't hypothetical —
`struct orb_metadata` had a layout-breaking change at v1.15 (dropped
`o_fields`, added `message_hash`, widened `o_id` from `uint8_t` to
`uint16_t`, added `o_queue`). Any future ABI break would silently
corrupt memory if our pinned submodule and the user's firmware
disagreed.

### Supported versions

- **Minimum supported PX4: v1.15.0.** This is where the current
  `orb_metadata` ABI was established; v1.14 and earlier are a different
  layout and are not supported.
- **Pinned for vendored snapshot / CI: v1.16.2** (latest stable release
  as of this writing). The relevant FFI surface is identical across
  v1.15, v1.16, and v1.17-rc2, so v1.16.2 is the safe anchor.
- CI shallow-clones the pinned tag for `just gen-sys` verification; no
  submodule in the repo.

### `build.rs` sanity check

When `PX4_AUTOPILOT_DIR` is set, `build.rs` must grep
`platforms/common/uORB/uORB.h` for `message_hash` and hard-fail the build
if missing. This converts a silent v1.14-era ABI mismatch into a loud
compile-time error.

## Work items

- [x] 02.1 — Add `crates/px4-sys/` with `links = "px4"` and a `build.rs`
      using `bindgen` 0.70
- [x] 02.2 — `wrapper.h` hand-authored: only the phase-02 FFI surface, no
      PX4 includes (avoids bindgen include-path gymnastics)
- [x] 02.3 — Vendored `bindings/bindings.rs` snapshot; `build.rs` falls
      back to it if bindgen/libclang is unavailable
- [x] 02.4 — C++ linkage: uORB and WorkQueue symbols wrapped via
      `wrapper.cpp` trampolines compiled by `cc`, gated on
      `PX4_RS_BUILD_TRAMPOLINES` (CMake-only signal)
- [x] 02.5 — `just gen-sys` → `xtask gen-sys` regenerates the vendored
      snapshot from `wrapper.h`
- [x] 02.6 — `#![no_std]` on `px4-sys` (declarations only, no impls)
- [ ] 02.7 — Per-topic `orb_metadata` blocks — deferred to phase 05
      (`px4-msg-codegen` owns the per-topic bindings)
- [x] 02.8 — `build.rs` rejects a pre-v1.15 `PX4_AUTOPILOT_DIR` by
      grepping `platforms/common/uORB/uORB.h` for the `orb_id_size_t`
      typedef (v1.15-only marker; robust against comment matches)
- [ ] 02.9 — CI workflow to shallow-clone v1.16.2 and verify the snapshot
      — pending a CI config file (no CI yet configured in-repo)

## Acceptance criteria

- [x] `cargo build -p px4-sys` succeeds on the host against the vendored
      snapshot (no PX4 checkout needed)
- [x] `cargo build -p px4-sys --target thumbv7em-none-eabihf` succeeds
      with `PX4_AUTOPILOT_DIR` set (without `PX4_RS_BUILD_TRAMPOLINES`
      the C++ is skipped — that's only for CMake-driven firmware builds)
- [x] The generated `bindings.rs` contains `orb_publish`,
      `hrt_call_every`, `orb_metadata`, `px4_log_modulename`, and the
      `px4_rs_wi_schedule_now` / `px4_rs_sub_cb_new` trampoline entries
      (PX4's C++ `WorkItem::ScheduleNow` is deliberately reached via
      the trampoline, not bound directly)
- [x] No `extern "C++"` ABI crossings in user-facing code — only through
      `wrapper.cpp` trampolines compiled by this crate
- [x] `build.rs` rejects a v1.14-or-earlier `PX4_AUTOPILOT_DIR` with a
      clear error message pointing at the phase-02 doc

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
