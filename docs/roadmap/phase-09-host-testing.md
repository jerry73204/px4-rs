# Phase 09 — Host-side mock + unit tests

**Goal**: Drive px4-rs crates in unit tests without a PX4 checkout.

**Status**: Done
**Priority**: P1
**Depends on**: Phase 02, Phase 04, Phase 06

## Design — pivoted from `px4-sys-mock` to per-crate `feature = "std"`

The original sketch proposed a sibling `px4-sys-mock` crate that
re-exported a subset of `px4-sys` symbols, gated by a `mock` feature
on `px4-sys`. As the runtime crates landed, the mock surface naturally
clustered into two distinct slices — work-item lifecycle (`px4-workqueue`)
and the uORB broker (`px4-uorb`) — and a shared third for HRT timers.
Each lives next to the FFI surface it mocks:

  - `px4-workqueue/src/ffi.rs::mock` — drives `px4_rs_wi_*` against an
    in-process `mpsc` dispatcher.
  - `px4-workqueue/src/hrt.rs::mock` — fans out a short-lived `std`
    thread per `hrt_call_after`; cancellation flips a flag.
  - `px4-uorb/src/ffi.rs::mock` — name-keyed broker over
    `Arc<Mutex<…>>`. `advertise` / `publish` / `sub_cb_*` all land here.

`px4-sys` itself stays no-std and FFI-only. The switch is a per-crate
`feature = "std"` rather than a workspace-wide feature on `px4-sys`.
This sidesteps proc-macro2/std issues for the codegen crates and keeps
each mock close to the surface it imitates — when a new FFI symbol
lands, the matching mock is obvious.

`hrt_absolute_time` isn't mocked yet; no current primitive needs it.
Add when the first call site appears.

## Work items

- [x] 09.1 — Per-crate host mocks land alongside the FFI surface they
      replace (`px4-workqueue/src/ffi.rs::mock`,
      `px4-workqueue/src/hrt.rs::mock`, `px4-uorb/src/ffi.rs::mock`).
      No standalone `px4-sys-mock` crate — see "Design" above.
- [x] 09.2 — `feature = "std"` lives on each consuming crate
      (`px4-workqueue`, `px4-uorb`, `px4-log`) instead of a `mock`
      feature on `px4-sys`. Activated workspace-wide by `just test`
      via `cargo test --all-features`.
- [x] 09.3 — Host integration tests cover every primitive:
        * Task spawn → wake → poll: `px4-workqueue/tests/basic.rs`
          and `tests/task_macro.rs`.
        * Sub/pub round-trip: `px4-uorb/tests/round_trip.rs` plus
          the phase-06.6 extensions tests.
        * Timer ticks: `px4-workqueue/tests/timer.rs`.
        * Notify cross-task signal: `px4-workqueue/tests/notify.rs`.
        * Channel SPSC + backpressure:
          `px4-workqueue/tests/channel.rs`.
        * `#[task]` compile-fail diagnostics:
          `px4-workqueue/tests/trybuild/`.
- [x] 09.4 — CI host-test matrix runs `just ci` on `ubuntu-latest`,
      `macos-latest` and `windows-latest`. `fail-fast: false` so a
      platform-specific regression doesn't mask coverage on the
      others. Job: `ci` in `.github/workflows/ci.yml`.

## Acceptance criteria

- [x] `cargo test --workspace --all-features` passes on the host with
      no PX4 checkout (verified locally with `env -u PX4_AUTOPILOT_DIR
      just ci`).
- [x] Tests cover every primitive in `px4-workqueue` and `px4-uorb`
      (see 09.3 for the matrix).
