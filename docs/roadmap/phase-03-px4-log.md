# Phase 03 — `px4-log` + panic handler

**Goal**: `log::info!` / `px4_log::info!` macros that emit via `PX4_INFO`,
plus a drop-in `#[panic_handler]`.

**Status**: Complete
**Priority**: P0 (every later crate depends on it for logging)
**Depends on**: Phase 02

## Work items

- [x] 03.1 — `px4_log::info!`, `warn!`, `err!`, `debug!` macros that render
      via `fmt::Write` into a 256-byte stack buffer and call
      `px4_log_modulename(level, MODULE_NAME, "%s", buf)`. Module name
      comes from a `module!("name")` declaration at the user's crate root.
- [x] 03.2 — `log` crate backend behind the `log` feature; `init()`
      registers a static `log::Log` impl via `set_logger_racy`
- [x] 03.3 — `#[panic_handler]` behind `panic-handler` feature; logs at
      PANIC level then calls `extern "C" { abort() }`
- [x] 03.4 — Host-side path behind the `std` feature — `__log_impl`
      routes to `eprintln!`, unit tests run without a PX4 link target

## Acceptance criteria

- [x] `px4_log::info!("x = {}", 42);` compiles in a `no_std` crate
- [x] Enabling the `log` feature routes `log::info!` through the same path
- [x] Panic handler formats `PanicInfo` with `Display` and calls
      libc `abort()`. Bare-metal target build (`thumbv7em-none-eabihf`)
      succeeds with `--features panic-handler`.
- [x] `cargo test -p px4-log --features std` passes on the host
      without a PX4 link target
