# Phase 07 — CMake integration + first Pixhawk module

**Goal**: A real Rust module blinks an LED or publishes a heartbeat on a
physical Pixhawk (or QEMU-PX4).

**Status**: Done — firmware build verified end-to-end by phase-11's SITL test suite
**Priority**: P0 (proves the toolchain end-to-end)
**Depends on**: Phase 02, Phase 03, Phase 04

## Work items

- [x] 07.1 — `cmake/px4-rust.cmake` exposes `px4_rust_module(NAME CRATE
      MANIFEST [ENTRY] [TARGET])`. Picks a Rust target from the board's
      `CONFIG_ARCH_CHIP_*` vars; sets `CARGO_TARGET_DIR` per module;
      drives `cargo build --release` with `PX4_AUTOPILOT_DIR` +
      `PX4_RS_BUILD_TRAMPOLINES=1`; tracks `Cargo.toml`, `src/**.rs`,
      and `build.rs` for re-builds; auto-generates a one-line C shim
      (`<NAME>_main` → `<ENTRY>`) so PX4's stock `px4_add_module()`
      sees a normal `int main(int, char**)` entry.
- [x] 07.2 — `examples/px4-rust-template/` ships `Cargo.toml` +
      `CMakeLists.txt` + `Kconfig` + `src/lib.rs`. The Rust side
      exports `px4_rust_template_main`, parses `start|stop|status`,
      and spawns one `#[task]`.
- [x] 07.3 — Target matrix verified for `thumbv7em-none-eabihf` and
      `thumbv8m.main-none-eabihf` (both example crates link clean).
      `riscv32imc-unknown-none-elf` works with the same recipe but
      isn't smoke-tested in CI.
- [x] 07.4 — `examples/heartbeat/` publishes an `Airspeed` topic in a
      loop with `yield_now` between iterations. (The phase doc
      originally specified `vehicle_command` at 1 Hz — substituted
      `Airspeed` for size and dropped the 1 Hz cap until phase 04's
      `Timer` lands.)
- [x] 07.5 — Linker / panic-handler / symbol-conflict guidance moved
      into [`docs/linking-into-px4.md`](../linking-into-px4.md).
      `#[panic_handler]` is now gated on `cfg(target_os = "none")` so
      the `panic-handler` feature can be enabled unconditionally
      without breaking host clippy. Multi-module caveat documented.

## Acceptance criteria

- [x] `make px4_sitl` with px4-rs modules linked in completes without
      errors — exercised by `tests/sitl/`'s cached SITL build.
- [x] `<module> start` on `pxh>` publishes visible uORB traffic —
      `tests/sitl/tests/smoke.rs::listener_airspeed_reads_back_rust_publish`
      drives the equivalent through `Px4Sitl::shell` and asserts on
      `listener airspeed` output.
- [x] Unit tests still green on the host (`cargo test --workspace`).
- [x] Both `libheartbeat.a` and `libpx4_rust_template.a` link clean
      for `thumbv7em-none-eabihf` and `thumbv8m.main-none-eabihf`,
      and `nm` confirms `<crate>_main` is exported with the right
      C ABI.

## References

- Pictorus's integration: https://www.docs.pictor.us/features/px4.html —
  same general approach (staticlib + CMake glue)
- PX4 `EXTERNAL_MODULES_LOCATION` docs:
  https://docs.px4.io/main/en/concept/module_template.html
