# Phase 09 — Host-side mock + unit tests

**Goal**: Drive px4-rs crates in unit tests without a PX4 checkout.

**Status**: Not Started
**Priority**: P1
**Depends on**: Phase 02, Phase 04, Phase 06

## Design

Host builds don't link real PX4. A mock `px4-sys-mock` provides:

- `ScheduleNow` pushes the `WorkItem*` onto a host-thread channel
- A host "WorkQueue driver" function drains the channel, calls `Run()`
- `orb_publish` / `orb_copy` use an in-process `Arc<Mutex<VecDeque>>`
- `hrt_absolute_time` reads `std::time::Instant`

## Work items

- [ ] 09.1 — `px4-sys-mock` crate (std, host-only) implementing a subset
      of `px4-sys` symbols
- [ ] 09.2 — Cargo feature `mock` on `px4-sys` that re-exports
      `px4-sys-mock` symbols
- [ ] 09.3 — `tests/host/` integration tests exercising:
    - Task spawn → wake → poll cycle
    - Sub/pub round-trip
    - Timer ticks
    - Notify cross-task signal
- [ ] 09.4 — CI matrix: run host tests on ubuntu, macos, windows

## Acceptance criteria

- [ ] `cargo test --workspace` passes on the host with no PX4 checkout
- [ ] Tests cover every primitive in `px4-workqueue` and `px4-uorb`
