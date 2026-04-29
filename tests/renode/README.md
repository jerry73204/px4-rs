# `tests/renode/` — Renode + NuttX e2e suite

Phase-13 sibling of `tests/sitl/`. Same fixture API, different boot
substrate: this suite drives PX4 + NuttX firmware running on an
emulated STM32H743 inside Renode, instead of the POSIX SITL build.

The Rust-side test bodies are nearly identical between the two
suites; what differs is whether the runtime under test is the
POSIX-thread variant of `px4::WorkQueue` or the actual NuttX
scheduler driving real ARM Cortex-M code.

## Status

End-to-end working. PX4-on-NuttX firmware boots on emulated
STM32H743, the fixture drives nsh through Renode's monitor, and 11
of 13 tests pass live (smoke, probe, pxh, e2e_smoke ×3, multi_wq,
pubsub, gyro_watch, panic, plus the existing fixture self-tests).

Two tests are `#[ignore]`d behind a precise Renode upstream bug
in `STM32_Timer.UpdateCaptureCompareTimer` — the wrap-fire path
of CC compare is broken (`CCR <= CNT` disables the channel
permanently). PX4's HRT writes a new `CCR` from inside the ISR,
which often lands behind the current `CNT` once Renode-time has
advanced; the timer fires once and never again. Affects
`example_hello_module` and `example_multi_task` (both rely on
`sleep(Duration)`). The full root-cause analysis, proposed
upstream patch, and a local-rebuild recipe live under work item
13.6 in
[`docs/roadmap/phase-13-renode-nuttx-e2e.md`](../../docs/roadmap/phase-13-renode-nuttx-e2e.md).

## Running

Two prerequisites:

1. **Renode** at the pinned version. On Debian/Ubuntu:
   ```sh
   just setup-renode
   ```
   The recipe downloads the pinned `.deb` from Antmicro's GitHub
   releases and `sudo apt install`s it. The version is set in the
   top-level `justfile` (`RENODE_VERSION`); override on the command
   line for one-off bumps:
   ```sh
   RENODE_VERSION=1.16.0 just setup-renode
   ```
   On macOS: `brew install --cask renode`. On Windows: the `.msi`
   from <https://renode.io/#downloads>.
2. **A built `px4_renode-h743_default.elf`** firmware binary. The
   board template is in `tests/renode/px4-board/`; copy it into
   PX4-Autopilot and build with `EXTERNAL_MODULES_LOCATION` pointing
   at the SITL externals tree (so `e2e_smoke`, `hello_module`, etc.
   end up linked into the firmware):

   ```sh
   bash tests/renode/scripts/setup-board.sh
   cd $PX4_AUTOPILOT_DIR
   EXTERNAL_MODULES_LOCATION=$HOME/repos/px4-rs/tests/sitl/px4-externals \
       make px4_renode-h743_default
   ```

   The shorter form is `just build-renode-firmware`.

Set both env vars and run from the workspace root:

```sh
RENODE=$(which renode) \
PX4_RENODE_FIRMWARE=$PX4_AUTOPILOT_DIR/build/px4_renode-h743_default/px4_renode-h743_default.elf \
PX4_RENODE_HAS_PX4=1 \
just test-renode
```

Without those env vars, every test reports `[SKIPPED]` and the suite
exits zero — same shape as `tests/sitl/`'s `ensure_px4!()`.

## Layout

```
tests/renode/
    Cargo.toml                              # standalone, not a workspace member
    rust-toolchain.toml                     # nightly pin (for parity with sister tests/)
    .config/nextest.toml                    # serial test-group, 60 s slow-timeout
    src/
        lib.rs                              # ensure_renode! macro, TestError
        process.rs                          # graceful_kill (mirrors tests/sitl)
        fixtures/
            mod.rs
            px4_renode.rs                   # Px4RenodeSitl: boot/shell/wait_for_log/wait_for_exit
    platforms/
        px4_renode_h743.repl                # extends Renode's stock stm32h743.repl
        px4_renode_h743.resc                # boots firmware, wires UART2 to a host pty
    tests/
        smoke.rs                            # parallels tests/sitl/tests/boot.rs
```

## Why this matters

POSIX SITL exercises the full Rust ↔ C link path against PX4's real
broker, scheduler and log formatter — but it builds for x86_64 Linux
and runs on pthreads. It can't catch:

- ARM Cortex-M codegen issues (alignment, ABI quirks)
- Atomics availability gaps (e.g. `AtomicU64` isn't lock-free on M0/M3)
- NuttX scheduler differences vs lockstep pthreads
- Real HRT timer ISR behaviour vs gettimeofday emulation

Phase 13 closes those, deterministically. See
[`docs/research/renode-vs-qemu.md`](../../docs/research/renode-vs-qemu.md)
for why the substrate is Renode rather than QEMU.

## Out of scope

Sensor / driver paths, real-time timing assertions, MAVLink against
a real ground station — those need hardware-in-the-loop. The phase-13
suite covers the runtime, scheduler, and ARM codegen tier; HITL is a
separate, opt-in track.
