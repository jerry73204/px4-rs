# Phase 07 — CMake integration + first Pixhawk module

**Goal**: A real Rust module blinks an LED or publishes a heartbeat on a
physical Pixhawk (or QEMU-PX4).

**Status**: Not Started
**Priority**: P0 (proves the toolchain end-to-end)
**Depends on**: Phase 02, Phase 03, Phase 04

## Work items

- [ ] 07.1 — `cmake/px4-rust.cmake` with `px4_rust_module(NAME CRATE ENTRY MANIFEST)`:
    - Invokes `cargo build --release --target <triple>`
    - Sets `CARGO_TARGET_DIR=<build>/rust-target/<name>`
    - Links `lib<name>.a` into the PX4 module target
    - Handles dependency tracking (re-build on `Cargo.toml` / `src/` changes)
- [ ] 07.2 — `px4-rust-template/` skeleton module (user copies this into
      their `EXTERNAL_MODULES_LOCATION` tree):
    - `CMakeLists.txt` calling `px4_rust_module`
    - `Cargo.toml`, `src/lib.rs` with a `#[task]` stub
- [ ] 07.3 — Target triple matrix: verify build on
      `thumbv7em-none-eabihf` (Pixhawk 4 / 5) and `thumbv8m.main-none-eabihf`
      (6X-RT)
- [ ] 07.4 — First real module: `heartbeat` that publishes a
      `vehicle_command` every 1 s
- [ ] 07.5 — Document the release-linker-trick (avoiding Rust stdlib
      symbols that clash with NuttX newlib)

## Acceptance criteria

- [ ] `make px4_sitl jmavsim` (or `make px4_fmu-v6x_default`) with the
      heartbeat module linked in completes without errors
- [ ] Running the module via `heartbeat start` on `pxh>` publishes
      visible uORB traffic (confirmed by `listener vehicle_command`)
- [ ] Unit tests still green on the host

## References

- Pictorus's integration: https://www.docs.pictor.us/features/px4.html —
  same general approach (staticlib + CMake glue)
- PX4 `EXTERNAL_MODULES_LOCATION` docs:
  https://docs.px4.io/main/en/concept/module_template.html
