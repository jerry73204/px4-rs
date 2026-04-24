# Linking px4-rs into a PX4 firmware build

PX4 modules are consumed by PX4's CMake tree. px4-rs modules ship as
Rust staticlibs with a C entry point, and PX4 sees them as just another
module: the [`px4_rust_module()`](../cmake/px4-rust.cmake) helper
builds the crate, generates a one-line C shim, and hands both to
PX4's stock `px4_add_module()` rule.

Two reference modules under `examples/` show the full layout end-to-end:

- [`examples/px4-rust-template/`](../examples/px4-rust-template/) —
  minimum viable Rust module (one task, one `info!` call).
- [`examples/heartbeat/`](../examples/heartbeat/) — uses
  `#[px4_message]` + `Publication<T>` to publish an `Airspeed` topic
  in a loop.

## Approach 1 — `EXTERNAL_MODULES_LOCATION` (recommended)

1. Add to your PX4 board config (`boards/.../default.px4board` or a
   downstream `CMakeLists.txt`):

   ```cmake
   set(EXTERNAL_MODULES_LOCATION "/path/to/your/px4_rust_modules")
   list(APPEND CONFIG_SHELL_CMDS "heartbeat start")
   ```

2. Inside `/path/to/your/px4_rust_modules/heartbeat/CMakeLists.txt`:

   ```cmake
   set(PX4_RS_DIR /path/to/px4-rs)            # absolute or relative
   include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

   px4_rust_module(
       NAME     heartbeat
       CRATE    heartbeat                       # Cargo package name
       MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
       # Optional:
       # ENTRY  heartbeat_main                  # default: ${CRATE}_main
       # TARGET thumbv7em-none-eabihf           # default: derived from board
   )
   ```

3. Build PX4 normally:

   ```sh
   make px4_fmu-v6x_default
   ```

   The rule invokes `cargo build --release --target <triple>` with
   `PX4_AUTOPILOT_DIR=<px4 source>` and `PX4_RS_BUILD_TRAMPOLINES=1`
   so `px4-sys` compiles its C++ trampolines against the live PX4
   headers, then links the resulting `libheartbeat.a` into the
   module target.

## Approach 2 — in-tree module

Same as Approach 1, but place the module directory directly under
`src/modules/` of your PX4 fork. Useful when upstreaming a Rust
module to PX4 itself.

## What `px4_rust_module()` does

1. Picks a Rust target triple from the board's `CONFIG_ARCH_CHIP_*`
   variables. Override with `TARGET ...` if needed.
2. Sets `CARGO_TARGET_DIR=<build>/rust-target/<NAME>` so concurrent
   module builds stay isolated and `make clean` works.
3. Runs `cargo build --release --target <triple> -p <CRATE>` with
   `PX4_AUTOPILOT_DIR` + `PX4_RS_BUILD_TRAMPOLINES=1` set in the
   environment.
4. Generates a tiny C shim that exports `<NAME>_main(int, char**)`
   and forwards to the Rust entry symbol (`<CRATE>_main` by default,
   or whatever `ENTRY` you pass).
5. Calls PX4's stock `px4_add_module()` with the shim as the source
   and the imported `lib<CRATE>.a` as a dependency.

## Target triples

| PX4 board family | Rust target |
| --- | --- |
| Cortex-M4 / M7 (FMU-v3..v5) | `thumbv7em-none-eabihf` |
| Cortex-M7 with TrustZone (FMU-v6X) | `thumbv8m.main-none-eabihf` |
| Pixhawk 6C-RT (RV32) | `riscv32imc-unknown-none-elf` |

`rust-toolchain.toml` already includes all three. NuttX-targeted
triples (`*-nuttx-*`, upstreamed in
[rust-lang/rust#130595](https://github.com/rust-lang/rust/pull/130595))
are an option for projects that want `std`; px4-rs is `no_std`
everywhere by default.

## Panic handler

A Rust staticlib needs a `#[panic_handler]`. `px4-log` provides one
behind the `panic-handler` feature; it formats via
`px4_log_modulename(PANIC, ...)` and calls libc `abort()`. The
attribute is gated on `target_os = "none"`, so enabling the feature
unconditionally is safe — host clippy / tests still link against
`std`'s panic handler.

**Multi-module caveat**: only one staticlib per firmware can define
`#[panic_handler]`. If you ship more than one Rust PX4 module in the
same build, enable `panic-handler` in exactly one of them and let the
others borrow it via the linker.

## Symbol conflicts

PX4 (NuttX) provides `printf`, `malloc`, `abort`, etc. Rust's `core`
doesn't need them, but some `crates.io` dependencies do — audit
before pulling them in. Prefer `core`-only crates and explicit
`#![no_std]`.

## Manual smoke test

Before a real PX4 build, verify the staticlib compiles on the host:

```sh
just check
just test
```

…and for a bare-metal target:

```sh
cargo build -p heartbeat --target thumbv7em-none-eabihf --release
arm-none-eabi-nm target/thumbv7em-none-eabihf/release/libheartbeat.a \
    | grep ' T heartbeat_main'
```

The `nm` step confirms the `<crate>_main` symbol is exported, which
is what the C shim forwards to.
