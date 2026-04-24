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

Full walkthrough, starting from a PX4 fork you've already been
working in. Five files touched total.

### Step 1 — get px4-rs

```sh
git clone https://github.com/aeon/px4-rs.git ~/src/px4-rs
# …or add it as a submodule of your PX4 fork, whichever you prefer.
```

### Step 2 — lay out the external-modules directory

PX4 expects a directory with a `src/` subtree:

```
~/my-externals/
    src/
        CMakeLists.txt            # step 4
        modules/
            heartbeat/            # step 3 — one dir per Rust module
                Cargo.toml
                CMakeLists.txt
                Kconfig
                rust-toolchain.toml
                src/lib.rs
                Airspeed.msg      # if the module uses #[px4_message]
```

### Step 3 — copy the example and edit four things

```sh
cp -r ~/src/px4-rs/examples/heartbeat \
      ~/my-externals/src/modules/heartbeat
```

Then edit the module's `Cargo.toml` to replace the `workspace = true`
dependency entries with explicit paths into your px4-rs checkout:

```diff
 [dependencies]
-px4-log              = { workspace = true, features = ["panic-handler"] }
-px4-sys              = { workspace = true }
-px4-uorb             = { workspace = true }
-px4-workqueue        = { workspace = true }
-px4-workqueue-macros = { workspace = true }
-px4-msg-macros       = { workspace = true }
+px4-log              = { path = "/home/you/src/px4-rs/crates/px4-log", features = ["panic-handler"] }
+px4-sys              = { path = "/home/you/src/px4-rs/crates/px4-sys" }
+px4-uorb             = { path = "/home/you/src/px4-rs/crates/px4-uorb" }
+px4-workqueue        = { path = "/home/you/src/px4-rs/crates/px4-workqueue" }
+px4-workqueue-macros = { path = "/home/you/src/px4-rs/crates/px4-workqueue-macros" }
+px4-msg-macros       = { path = "/home/you/src/px4-rs/crates/px4-msg-macros" }
```

Copy `rust-toolchain.toml` in too — the examples need nightly for the
`type_alias_impl_trait` feature used by the `#[task]` macro:

```sh
cp ~/src/px4-rs/rust-toolchain.toml \
   ~/my-externals/src/modules/heartbeat/rust-toolchain.toml
```

Finally, edit the module's `CMakeLists.txt` to point `PX4_RS_DIR` at
the cloned px4-rs location:

```cmake
set(PX4_RS_DIR /home/you/src/px4-rs)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     heartbeat                          # shell command name
    CRATE    heartbeat                          # Cargo package name
    MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
    # Optional:
    # ENTRY  some_other_symbol                  # default: ${CRATE}_main
    # TARGET thumbv7em-none-eabihf              # default: derived from board
)
```

### Step 4 — write the parent `src/CMakeLists.txt`

PX4 looks for `${EXTERNAL_MODULES_LOCATION}/src/CMakeLists.txt` and
reads `config_module_list_external` out of it:

```cmake
set(config_module_list_external
    modules/heartbeat
    PARENT_SCOPE
)
```

### Step 5 — wire it into your board

In your PX4 fork's board config (e.g.
`boards/px4/fmu-v6x/default.px4board`, or your custom board):

```cmake
set(EXTERNAL_MODULES_LOCATION "/home/you/my-externals")
list(APPEND CONFIG_SHELL_CMDS "heartbeat start")
```

Also enable the module in Kconfig:

```sh
make px4_fmu-v6x_default boardguiconfig
# navigate to "External Modules" and enable heartbeat (MODULES_HEARTBEAT=y)
```

### Step 6 — build

```sh
make px4_fmu-v6x_default
```

The `px4_rust_module()` rule invokes `cargo build --release --target
<triple>` with `PX4_AUTOPILOT_DIR=<px4 source>` and
`PX4_RS_BUILD_TRAMPOLINES=1`
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
