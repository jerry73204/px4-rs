# Linking px4-rs into a PX4 firmware build

PX4 modules are consumed by PX4's CMake tree. px4-rs modules ship as Rust
staticlibs with a C entry point, and PX4 sees them as just another module.

## Approach 1 — `EXTERNAL_MODULES_LOCATION` (recommended)

1. Add to your PX4 board config (`boards/.../default.px4board` or a
   downstream `CMakeLists.txt`):

   ```cmake
   list(APPEND CONFIG_SHELL_CMDS "my_rust_module start")
   set(EXTERNAL_MODULES_LOCATION "/path/to/your/px4_rust_modules")
   ```

2. Inside `/path/to/your/px4_rust_modules/my_rust_module/CMakeLists.txt`:

   ```cmake
   include(${CMAKE_CURRENT_LIST_DIR}/../px4-rs.cmake)   # this repo's cmake/
   px4_rust_module(
       NAME     my_rust_module
       CRATE    my_rust_module
       ENTRY    my_rust_module_main
       MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
   )
   ```

3. Build PX4 normally (`make px4_fmu-v6x_default`). The rule invokes
   `cargo build --release --target <px4-triple>` and links the resulting
   `libmy_rust_module.a` into the module's `px4_add_module()` target.

The `px4_rust_module()` function will live in
[`cmake/px4-rust.cmake`](../cmake/px4-rust.cmake) (to be written in the
CMake integration phase).

## Approach 2 — in-tree module

Same as Approach 1 but placed directly in `src/modules/` of your PX4 fork.
Useful when upstreaming a Rust module to PX4 itself. No difference in the
Cargo side — just the CMake module placement.

## Target triples

PX4 runs on:

- `arm-none-eabi` — Cortex-M4/M7 boards (`thumbv7em-none-eabihf`)
- `aarch64-none-elf` — some newer boards (`aarch64-unknown-none`)
- `riscv32imc-unknown-none-elf` — Pixhawk 6C-RT (Rust tier 3)

For NuttX targets with the Rust tier-3 `*-nuttx-*` triples
(upstreamed in rust-lang/rust#130595), `std` is optional. px4-rs is
`no_std` everywhere by default; `alloc` is opt-in per crate feature.

## Panic handler

PX4's C++ modules don't need one; Rust staticlibs do. `px4-log` provides
`px4_log::panic_handler!` which formats via `PX4_ERR` and calls
`abort()`. Users can override with their own by `#![panic_handler]` in
their module crate.

## Symbol conflicts

PX4 defines `printf`, `malloc`, etc. Rust's libcore doesn't need them,
but some dependencies (e.g. `log`'s default formatter) do. Audit before
pulling in dependencies; prefer `core`-only crates.

## Build caching

Cargo caches inside `target/`. CMake calls cargo with a stable
`CARGO_TARGET_DIR=<px4-build>/rust-target/<module-name>` so that Clean
works and concurrent module builds don't stomp each other.

## Manual smoke test

Before a real PX4 build, verify the staticlib compiles on the host:

```
just build
just test
```

and for the target:

```
just build-target thumbv7em-none-eabihf
```

Target builds exclude `xtask` (which uses `std`); see `justfile`.
