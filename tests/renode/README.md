# `tests/renode/` — Renode + NuttX e2e suite

Phase-13 sibling of `tests/sitl/`. Same fixture API, different boot
substrate: this suite drives PX4 + NuttX firmware running on an
emulated STM32H743 inside Renode, instead of the POSIX SITL build.

The Rust-side test bodies are nearly identical between the two
suites; what differs is whether the runtime under test is the
POSIX-thread variant of `px4::WorkQueue` or the actual NuttX
scheduler driving real ARM Cortex-M code.

## Status

The Rust infrastructure is complete: workspace skeleton,
`Px4RenodeSitl` fixture, `.repl` + `.resc` platform files, three
smoke tests, CI integration, `just test-renode` recipe.

The remaining work is **work item 13.1** in
[`docs/roadmap/phase-13-renode-nuttx-e2e.md`](../../docs/roadmap/phase-13-renode-nuttx-e2e.md):
authoring a `px4_renode_h743` PX4 board config that builds a
no-peripheral NuttX firmware suitable for booting in Renode. Without
it, every test in this crate skip-returns via `ensure_renode!()`.

## Running

Two prerequisites:

1. **Renode**. Either install the binary
   (`apt install renode` on Debian/Ubuntu, or download from
   <https://renode.io/#downloads>), or use the official Docker image.
2. **A built `px4_renode_h743.elf`** firmware binary — the output of
   `make px4_renode_h743_default` once the PX4 board config lands.

Set both env vars and run from the workspace root:

```sh
RENODE=$(which renode) \
PX4_RENODE_FIRMWARE=$HOME/repos/PX4-Autopilot/build/px4_renode_h743_default/px4_renode_h743.elf \
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
