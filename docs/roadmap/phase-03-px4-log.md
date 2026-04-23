# Phase 03 — `px4-log` + panic handler

**Goal**: `log::info!` / `px4_log::info!` macros that emit via `PX4_INFO`,
plus a drop-in `#[panic_handler]`.

**Status**: Not Started
**Priority**: P0 (every later crate depends on it for logging)
**Depends on**: Phase 02

## Work items

- [ ] 03.1 — `px4_log::info!`, `warn!`, `err!`, `debug!` macros that expand
      to `px4_sys::_PX4_LOG_IMPL` with module-name / file / line
- [ ] 03.2 — Optional `log` crate backend via a feature flag
- [ ] 03.3 — `#[panic_handler]` behind `panic-handler` feature (opt-in —
      PX4 C++ modules in the same binary bring their own)
- [ ] 03.4 — Host-side no-op implementation so unit tests don't need PX4

## Acceptance criteria

- [ ] `px4_log::info!("x = {}", 42);` compiles in a `no_std` crate
- [ ] Enabling the `log` feature routes `log::info!` through the same path
- [ ] Panic handler formats with `{:?}` on `PanicInfo` and calls
      `px4_sys::abort()` (or `loop {}` on `no_std` hosted targets)
