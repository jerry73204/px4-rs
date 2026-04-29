# px4-rs — repo guide for Claude

Rust async framework for PX4 modules. Treats PX4's `WorkQueue` as a first-class async executor: `Waker::wake()` maps directly to `WorkItem::ScheduleNow()`, so `#[task]` compiles to a native PX4 `ScheduledWorkItem` with no second scheduler on top.

## Layout

- `crates/` — Cargo workspace members. Library crates (no `panic_handler!()`):
  - `px4-sys` — bindgen FFI (uORB, WorkQueue, hrt, log)
  - `px4-log` — `PX4_INFO`/`WARN`/`ERR` + `log` adapter
  - `px4-workqueue` (+ `-macros`) — `#[task]`, `WorkItemCell`, `RawWaker → ScheduleNow`, `Timer`
  - `px4-msg-codegen` (+ `px4-msg-macros`) — `#[px4_message("msg/foo.msg")]`
  - `px4-uorb` — typed `Publication<M>` / `Subscription<M>`
  - `px4` — umbrella facade + `#[px4::main]` (`px4-macros`)
- `examples/` — standalone PX4 modules (NOT workspace members; each has its own Cargo.toml + `panic_handler!()`).
- `tests/sitl/` — POSIX SITL e2e (excluded from workspace; nightly + nextest).
- `tests/renode/` — Renode + NuttX-on-H743 e2e (excluded; pulls Renode at pinned `RENODE_VERSION`).
- `docs/roadmap/phase-NN-*.md` — work-by-phase log; commit messages reference the phase id (e.g., `phase-13.1: ...`).
- `xtask/` — repo automation, invoked via `cargo xtask` from `just`.

## Conventions

- Workspace excludes `examples/*` and `tests/{sitl,renode}` deliberately — copying a template module gives a standalone, buildable directory; SITL/Renode tests link `panic_handler!()` that would collide with `std`. Do not add them back to the workspace.
- Edition 2024 (`rust-version = "1.85"`).
- Commit subjects use `phase-NN.M: short description`. Body explains *why*. Existing commits set the tone — match them.
- Touching docs: only update the phase doc that currently describes the work. Don't re-narrate finished phases.
- One bug fix per commit. Don't bundle drive-by cleanups.

## Environment

- `PX4_AUTOPILOT_DIR` (defaults to `../PX4-Autopilot`) — codegen, SITL, Renode all need it.
- `RENODE` — path to the Renode binary; `just setup-renode` installs the pinned version on Debian/Ubuntu.
- `PX4_RENODE_FIRMWARE` — ELF for `tests/renode/` to boot.
- `PX4_RENODE_HAS_PX4=1` — un-skip the shell-driven tests when firmware contains PX4 modules (not just bare nsh).

## Common commands

```
just setup        # toolchain + Renode binary
just check        # fmt + clippy
just test         # host unit tests
just test-sitl    # POSIX SITL suite (needs PX4_AUTOPILOT_DIR)
just test-renode  # Renode + NuttX-H743 suite (needs PX4_RENODE_FIRMWARE)
just gen-msgs     # regenerate Rust bindings from msg/*.msg
```

## Renode tests — invoking directly

```
RENODE=/usr/bin/renode \
PX4_RENODE_FIRMWARE=$PX4_AUTOPILOT_DIR/build/px4_renode-h743_default/px4_renode-h743_default.elf \
PX4_RENODE_HAS_PX4=1 \
just test-renode
```

To rebuild the renode-h743 firmware (with the SITL externals linked in) after editing `tests/renode/px4-board/`:

```
just build-renode-firmware
```

## Known runtime gaps

- **Renode `STM32_Timer` doesn't fire CC compare-match after CCR <= CNT.** Affects two `tests/renode/` tests (`example_hello_module`, `example_multi_task`) that sleep for periods triggering the wrap path. `#[ignore]`d with explanatory file-level docs and a precise root cause + proposed upstream patch in `docs/roadmap/phase-13-renode-nuttx-e2e.md` (work item 13.6). Local rebuild needs `dotnet-sdk-8.0` + `sudo` to swap `/opt/renode/bin/Infrastructure.dll`; cleanest path is an upstream PR to `renode/renode-infrastructure`.
- All other phase-13 runtime issues are documented in the same place and have landed fixes (USART3 DMA disabled, `yield_now()` calls `usleep(1)` on NuttX, pty pairing replaced by Renode-monitor `WriteLine` for shell input, etc.).

## Pointers

- Architecture: `docs/architecture.md`
- Linking into PX4: `docs/linking-into-px4.md`
- Async model: `docs/async-model.md`
- `#[task]` macro: `docs/task-macro.md`
- Phase-13 (Renode + NuttX e2e): `docs/roadmap/phase-13-renode-nuttx-e2e.md`
