# Phase 12 — `px4` umbrella crate + `#[px4::main]`

**Goal**: Hide C-style entry-point boilerplate behind a Rusty
`#[px4::main]` attribute and unify the import surface under a single
`px4::` facade. End state: a user can write a complete PX4 Rust
module with one `use px4::*` and ~15 lines of code.

**Status**: Done
**Priority**: P1 (the largest lever on day-1 user experience)
**Depends on**: Phase 03 (`px4-log`), Phase 04 (`px4-workqueue`),
Phase 06 (`px4-uorb`)

## Motivation

Today every module copies the same shape:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn hello_module_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => match ticker::try_spawn() { /* … */ }
        Some(b"status") => { info!("running"); 0 }
        Some(b"stop") => { info!("stop is a no-op"); 0 }
        _ => { err!("usage: …"); 1 }
    }
}

fn parse_first_arg<'a>(argc: c_int, argv: *mut *mut c_char) -> Option<&'a [u8]> {
    /* 15 lines of unsafe pointer math */
}
```

…and pulls imports from `px4_log`, `px4_workqueue`,
`px4_workqueue_macros`, `px4_uorb`, `px4_msg_macros`, `px4_sys`. The
`unsafe`-laden entry-point and the import sprawl are both gratuitous.

## What PX4 actually expects from the entry-point return value

Confirmed by reading PX4's POSIX dispatcher
([`platforms/posix/src/px4/common/px4_daemon/pxh.cpp:113-119`](https://github.com/PX4/PX4-Autopilot/blob/v1.16.2/platforms/posix/src/px4/common/px4_daemon/pxh.cpp)):

```cpp
int retval = _apps[command](words.size(), arg);
if (retval) {
    if (!silently_fail) {
        printf("Command '%s' failed, returned %d.\n",
               command.c_str(), retval);
    }
}
```

- The integer return is opaque beyond `0 = success / non-zero =
  failure`. No convention for error codes (no 2 = usage, etc.).
- The dispatcher prints a generic framing line on non-zero. The real
  human-readable error is the module's responsibility, by convention
  through `px4_log_modulename` (i.e. our existing `err!` macro).
- That justifies the Termination shape: `Err(e: Display)` →
  `err!("{e}")` + return 1.

## Design — resolved

### Crate placement

Add two new crates to the workspace:

- **`px4-macros`** (`proc-macro = true`) — homes `#[main]`. Sits
  alongside the existing `px4-workqueue-macros` and `px4-msg-macros`;
  no consolidation in this phase, but designed so future proc-macros
  land here unless they're tightly coupled to a single runtime crate.
- **`px4`** (lib) — facade. Re-exports user-facing items from the
  workspace. The single `use px4::…` import users hit first.

`px4-log`, `px4-workqueue`, `px4-uorb`, `px4-sys` stay focused on
their slice. `px4` is documentation glue and one-liner re-exports;
no new logic.

### `#[px4::main]` shape

```rust
#![no_std]
#![feature(type_alias_impl_trait)]

use px4::{main, info, panic_handler, task, Args};

panic_handler!();

#[task(wq = "lp_default")]
async fn ticker() {
    /* … */
}

#[main]                                        // name = CARGO_PKG_NAME
fn main(args: Args) -> Result<(), &'static str> {
    match args.subcommand() {
        Some(b"start")  => {
            ticker::try_spawn().map_err(|_| "already running")?;
            info!("started");
            Ok(())
        }
        Some(b"status") => { info!("running"); Ok(()) }
        Some(b"stop")   => Ok(()),
        _ => Err("usage: hello_module {start|stop|status}"),
    }
}
```

The macro:

1. Reads `name = "..."` if given, else `env!("CARGO_PKG_NAME")` with
   `-` translated to `_`.
2. Emits a `MODULE_NAME: &'static CStr` const at the call site (so
   `info!`/`err!` resolve it). Subsumes `module!()` for crates that
   adopt `#[main]`.
3. Wraps the user fn in an `extern "C" fn <name>_main(int, char**)`
   that:
   - Builds an `Args` with `Args::from_raw(argc, argv)`.
   - Calls the user fn with 0 or 1 arg depending on its signature.
   - Converts the return through `ModuleResult::into_c_int(...)`.
4. Validates the user's signature: not `async`, no `self`, ≤ 1 arg.

### `Args` — already designed

Lives in `px4-log` (next to the other "this is a PX4 module"
boilerplate it grew up with: `module!`, `panic_handler!`). Iterator
over `&CStr` with a `subcommand()` shortcut returning `&[u8]` for
the universal `match args.subcommand() { Some(b"start") => … }`
dispatch. `Copy`, zero-cost, no allocation.

### `ModuleResult` trait

```rust
pub trait ModuleResult {
    fn into_c_int(self) -> c_int;
}

impl ModuleResult for () { fn into_c_int(self) -> c_int { 0 } }
impl ModuleResult for c_int { fn into_c_int(self) -> c_int { self } }

impl<T: ModuleResult, E: core::fmt::Display> ModuleResult for Result<T, E> {
    fn into_c_int(self) -> c_int {
        match self {
            Ok(t)  => t.into_c_int(),
            Err(e) => { ::px4_log::err!("{e}"); 1 }
        }
    }
}
```

Recursive Result handles `Result<(), &'static str>`,
`Result<c_int, MyErr>`, and any custom return type that implements
the trait.

Lives in `px4-log` (open trait — anyone can `impl ModuleResult for
MyType`).

### `px4` umbrella surface (initial)

```rust
// px4 = a one-stop import. Categories collapsed for ergonomics.

// Module entry
pub use px4_log::{Args, ArgsIter, ModuleResult};
pub use px4_macros::main;

// Logging
pub use px4_log::{Level, debug, err, info, module, panic_handler, warn};

// Async runtime
pub use px4_workqueue::{
    AtomicWaker, Channel, Notified, Notify, Sleep, SpawnError, SpawnToken,
    WorkItemCell, WqConfig, sleep, task, wq_configurations,
};

// uORB pub/sub
pub use px4_uorb::{
    OrbMetadata, PubError, Publication, Subscription, UorbTopic,
};
pub use px4_msg_macros::px4_message;

// Raw FFI for advanced users
pub use px4_sys as sys;
```

`Channel::recv` returns `px4::workqueue::Recv` — the internal future
types stay namespaced, not flattened, to avoid the
`Recv` (channel) vs `Recv` (subscription) collision. Same for `Send`
(channel future) which would shadow `core::marker::Send`.

## Work items

- [x] 12.1 — `px4-macros` proc-macro crate with `#[main]`. Parses
      `name = "..."` (defaulting to `CARGO_PKG_NAME` with `-`→`_`),
      validates the signature (sync, no `self`, ≤ 1 arg), emits the
      `MODULE_NAME` const at the call site, and wraps the user fn in
      an `extern "C" fn <name>_main(int, char**)` that runs through
      `ModuleResult::into_c_int`.
- [x] 12.2 — `Args` iterator + `ModuleResult` trait in `px4-log`.
      Re-exported from the umbrella. Five Args tests + five
      ModuleResult tests cover the contract.
- [x] 12.3 — `px4` umbrella facade. Re-exports the user-facing API
      under one namespace. Three sub-types that collide on simple
      names (`Recv` × 2, `Send` shadow of `core::marker::Send`)
      stay namespaced under `px4::workqueue::*` / `px4::uorb::*`.
      Forwards `feature = "std"` to the consuming crates.
- [x] 12.3a (extra) — `proc-macro-crate` integration in
      `px4-workqueue-macros` and `px4-msg-macros`: emitted paths
      now resolve through `::px4` when the user has only the
      umbrella, or directly through `::px4_workqueue` /
      `::px4_uorb` / `::px4_sys` when the user pins those. Without
      this, `#[task]` and `#[px4_message]` would have failed under
      the umbrella because they emit absolute crate-rooted paths.
      `px4-msg-codegen` grew an `EmitPaths` overrideable struct +
      `generate_with_paths` API to support this.
- [x] 12.4 — Migrated `examples/hello_module/`,
      `examples/multi_task/` and `examples/gyro_watch/` to the new
      API. Each module dropped ~30 lines: `parse_first_arg`,
      `extern "C"` wrapper, manual exit-code returns. Cargo.toml
      now reads `px4 = { … }` only — the umbrella absorbs the
      previous fan-out across `px4-log`, `px4-sys`, `px4-workqueue`,
      `px4-workqueue-macros`, `px4-uorb`, `px4-msg-macros`.
- [x] 12.5 — `trybuild` compile-fail tests for `#[main]` in
      `crates/px4/tests/trybuild/fail/`: `async_fn`, `self_param`,
      `too_many_args`, `unknown_arg`, `bad_return_type`. Each `.rs`
      has a paired `.stderr` snapshot pinning the diagnostic span.
- [x] 12.6 — SITL e2e suite (12 tests) passes against the migrated
      examples. The `example_hello_module`, `example_multi_task`
      and `example_gyro_watch` tests check log output — they pass
      unchanged, confirming the macro-emitted C entry point
      behaves identically to the hand-rolled one it replaced.

## Acceptance criteria

- [x] `cargo build -p px4` succeeds against the host fallback (no
      `PX4_AUTOPILOT_DIR` needed).
- [x] `cargo build -p px4 --target thumbv7em-none-eabihf` succeeds.
- [x] All three migrated examples build for host + thumbv7em and
      drop ≥ 25 lines each vs the pre-migration shape (hello_module
      went from 89 → 51, multi_task 117 → 80, gyro_watch 125 → 86).
- [x] `cd tests/sitl && cargo nextest run` passes 12/12, including
      the three `example_*` tests (~30 s warm).
- [x] `#[main]` rejects `async fn`, `self`, > 1 arg, unknown
      attribute args, and non-`ModuleResult` return types — five
      pinned diagnostics in `crates/px4/tests/trybuild/fail/`.
- [x] `cargo doc -p px4 --no-deps` produces a complete crate page
      whose top-level rustdoc is a runnable hello-world.

## Out of scope

- Migrating `examples/heartbeat/` or `examples/px4-rust-template/`.
  Those are pre-existing references; stable shape, no churn.
- Migrating `tests/sitl/px4-externals/src/modules/`. Internal test
  fixtures; lower-level API is fine for them.
- Consolidating `px4-workqueue-macros` and `px4-msg-macros` into
  `px4-macros`. Possible follow-up; this phase only adds the new
  proc-macro home.
- Future crates (`px4-params`, `px4-hrt`, `px4-cli`). Each is its
  own phase and slots into the umbrella when it lands.

## Risks

- **Macro re-export of `module!`**. `#[main]` emits `MODULE_NAME` for
  the call-site logging macros. A user who *also* writes
  `module!("foo")` gets a duplicate-const compile error. Mitigation:
  document clearly that `#[main]` subsumes `module!()`. The error
  surface is at least loud and immediate.
- **Name collisions in the umbrella.** `Recv` lives in both
  `px4-workqueue` (channel) and `px4-uorb` (subscription). Solved
  by *not* flattening those — leave them at
  `px4::workqueue::Recv` / `px4::uorb::Recv`. User-facing nouns
  (`Channel`, `Subscription`) move to the top level.
- **`ModuleResult` for `&str` ergonomics**. `Err("usage: …")` works
  out of the box because `&str: Display`. But `Result<(), io::Error>`
  in std builds also works — fine.
