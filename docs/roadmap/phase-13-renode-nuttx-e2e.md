# Phase 13 — Renode + NuttX end-to-end test track

**Goal**: A second e2e suite that boots PX4 + NuttX on emulated ARM
Cortex-M and runs the same `e2e_*` test bodies that
`tests/sitl/` already runs against POSIX SITL. Closes the ARM-codegen
+ NuttX-scheduler + interrupt-timing gap that POSIX SITL leaves open.

**Status**: Infrastructure + live NuttX-on-H743 boot proven; full PX4 firmware (13.1) tracked separately
**Priority**: P1 (any phase that changes the runtime should run on
a target-shaped substrate before merge)
**Depends on**: Phase 11 (the SITL fixture shape we mirror), Phase 12
(`px4` umbrella + `#[px4::main]` examples are what these tests
exercise)

## Why this exists

`tests/sitl/` is great at what it covers — the Rust ↔ C ABI, the
real uORB broker, the real `WorkQueue` scheduler — but it runs
against PX4's POSIX build on x86_64 Linux. The bugs it can't see:

- ARM Cortex-M codegen issues (alignment, ABI quirks, atomics
  availability — `AtomicU64` isn't lock-free on Cortex-M0/M3).
- NuttX scheduler differences vs pthread + lockstep.
- Real HRT timer ISR behaviour vs gettimeofday emulation.
- Linker-script and NuttX-kernel-hook integration.

Phase 13 closes all four. See [research note](../research/renode-vs-qemu.md)
for why the substrate is Renode rather than QEMU — short version:
NuttX-on-Cortex-M is documented working on Renode for the exact
SoC families Pixhawk uses, QEMU's Cortex-M coverage is two TI
boards.

What phase 13 does **not** close: real sensor driver paths, real
PWM timing, MAVLink against a real ground station. Those need
hardware-in-the-loop, separately.

## Target board

**Custom `px4_renode_h743` board config**, modelled after
`boards/px4/fmu-v6x/` but stripped of sensor drivers. Rationale:

- FMU-v6X uses STM32H743VIT6 (Cortex-M7).
- Renode ships `platforms/cpus/stm32h743.repl` and
  `platforms/boards/nucleo_h753zi.repl` out of the box — same SoC
  family.
- NuttX has a working Renode port for `Nucleo-H743ZI` documented
  upstream
  ([apache.org](https://nuttx.apache.org/docs/12.8.0/guides/renode.html)).
  Only one config flag tweak needed:
  `CONFIG_STM32H7_PWR_IGNORE_ACTVOSRDY=y`.
- The PX4-side board config keeps NuttX, the WorkQueue manager,
  uORB, the param subsystem, the `pxh` shell over UART — and drops
  every driver that touches a peripheral Renode doesn't model
  (sensors on i2c/spi, BARO/MAG/GYRO, RC input, FMU PWM out).

Net effect: a "PX4 on H7 with no real-world I/O, just shell + WQ +
uORB + log + Rust modules" build. Big enough to exercise the
runtime, small enough to fit phase 13's CI budget.

## Architecture

```
tests/renode/                                # standalone workspace, mirrors tests/sitl/
    Cargo.toml
    rust-toolchain.toml
    .config/nextest.toml                     # serial test-group, like SITL
    src/
        lib.rs
        fixtures/
            mod.rs
            renode_proc.rs                   # spawn renode --console, drain Monitor I/O
            uart_pty.rs                      # connect to Renode's pty, line-tail loop
            px4_renode.rs                    # Px4RenodeSitl: boot/shell/wait_for_log/wait_for_exit
        process.rs                           # graceful_kill (reuse SITL helper shape)
    platforms/
        px4_renode_h743.repl                 # extends stock stm32h743.repl
        px4_renode_h743.resc                 # boots firmware, opens UART pty
    px4-board/
        nuttx-config/                        # PX4 board config: NuttX defconfig + Kbuild
        default.px4board                     # the `px4_renode_h743` board manifest
    px4-externals/                           # same modules tests/sitl/px4-externals ships
        ...                                  # symlink or copy from tests/sitl/
    tests/
        smoke.rs                             # mirror of tests/sitl/tests/smoke.rs
        pubsub.rs
        panic.rs
        multi_wq.rs
        example_*.rs
```

## The fixture API

Same shape as `Px4Sitl`, intentionally — phase 13 changes the boot
substrate, not the test surface.

```rust
pub struct Px4RenodeSitl { /* … */ }

impl Px4RenodeSitl {
    pub fn boot() -> Result<Self>;
    pub fn shell(&self, cmd: &str) -> Result<String>;
    pub fn wait_for_log(&self, pat: &str, timeout: Duration) -> Result<String>;
    pub fn wait_for_exit(&self, timeout: Duration) -> Option<ExitStatus>;
}
```

Internally:

- `boot()` spawns `renode --console -e "include @<path>/px4_renode_h743.resc"`
  in its own process group. The `.resc` script wires UART2 to a pty
  Renode prints to its console; we capture that pty path, open it
  read-side, and tail it.
- `shell(cmd)` writes `cmd\n` to the pty and reads until prompt.
- `wait_for_log` and `wait_for_exit` lift directly from the SITL
  fixture's logic — only the underlying I/O source changes.

Existing test bodies (`tests/sitl/tests/*.rs`) are nearly portable.
Three timing assumptions need adjusting for virtual time: the
`hello_module_ticks_at_least_twice` test, the panic-test exit
window, and the `consumer woke, count=2` window. Each gets a
`tests/renode/`-side variant that Renode-advances explicit time
ticks rather than wall-clock sleeping.

## Determinism dividend

Renode runs on a virtual clock the test drives explicitly. The
`airspeed_topic_appears_in_uorb_status` flake we hit in SITL — race
between PX4's stock `airspeed_selector` registering and our test
poll — can't happen here. Two sub-benefits:

- Test runtime drops below SITL's because nobody's sleeping a real
  200 ms.
- Per-test reproducibility: same machine state every run.

## Work items

- [/] **13.1-lite** — Stock NuttX `nucleo-h743zi:nsh` boots on
      Renode, validated end-to-end. Path:
      `tools/configure.sh nucleo-h743zi:nsh` + add
      `CONFIG_STM32H7_PWR_IGNORE_ACTVOSRDY=y` (note: this flag
      doesn't actually exist in PX4's NuttX fork — handled via a
      Renode-side PWR mock instead, see below) + `make`. Result:
      `nuttx` ELF (~1.3 MB) that Renode loads and runs through to
      NuttX's `NuttShell` banner + `nsh>` prompt in 1.7 s warm.
      The `tests/renode/tests/smoke.rs::fixture_boots_to_nuttx_banner`
      test gates the boot path — passes live in CI when
      `PX4_RENODE_FIRMWARE` is set to the NuttX ELF.
- [ ] **13.1** — Full PX4-on-NuttX board config (`px4_renode_h743`):
      a custom PX4 board branched from `boards/px4/fmu-v6x/`,
      sensor + actuator drivers stripped, NuttX defconfig from the
      working 13.1-lite baseline. Once it produces a firmware,
      `tests/renode/tests/pxh.rs` gates flip to live — the gating
      env var is `PX4_RENODE_HAS_PX4=1` (uorb / shell tests).
      **Open issue blocking shell-driven tests on bare NuttX**:
      stock `nucleo-h743zi:nsh` panics shortly after the prompt
      via `irq_unexpected_isr → PANIC()` on IRQ 120 (MDIOS) — an
      interrupt Renode's STM32H7 model fires from reset-default
      state that NuttX hasn't registered a handler for. PX4 board
      configs can disable MDIOS at the Kconfig level, dodging the
      issue. Tracked here rather than as a blocker for
      infrastructure validation.
- [x] **13.2** — `tests/renode/` workspace skeleton: standalone
      `Cargo.toml`, `rust-toolchain.toml`, `.config/nextest.toml`
      with a `renode` test-group capped at 1 thread.
- [x] **13.3** — `platforms/px4_renode_h743.repl` extends Renode's
      stock `stm32h743.repl` (vendored fragment with the upstream
      MIT header preserved). Companion `px4_renode_h743.resc`
      wires USART2 to a host-side pty and loads the firmware ELF.
- [x] **13.4** — `Px4RenodeSitl` fixture lands at
      `tests/renode/src/fixtures/px4_renode.rs` with the
      `boot / shell / wait_for_log / wait_for_exit` API exactly
      mirroring `Px4Sitl`. Renode subprocess management, pty
      master tail thread, RAII teardown via SIGTERM-then-SIGKILL.
      Compiles clean; tests skip-pass without `RENODE` /
      `PX4_RENODE_FIRMWARE`.
      Also ships [`probe_platform`] + the lighter
      `ensure_renode_binary!()` macro: a non-interactive
      Renode-spawn that loads the `.repl` and quits, used by
      `tests/probe.rs` for live coverage that doesn't need 13.1.
- [ ] **13.5** — Reuse `tests/sitl/px4-externals/`. Plumbed once
      13.1 lands; trivial once the firmware build is in place.
- [ ] **13.6** — Port the existing test bodies. Two are stubbed in
      `tests/renode/tests/smoke.rs` already (boot probe + uorb
      status), with `ensure_renode!()` skip detection. The rest
      port wholesale once the firmware actually boots on Renode.
      Timer-bound ones may want a Renode-time-advance helper.
- [x] **13.7** — CI track. `renode-e2e` job in
      `.github/workflows/ci.yml` runs `just setup-renode`
      (installing the pinned `.deb`), then `cargo test` against
      `tests/renode/`. With `RENODE` set, `tests/probe.rs`
      executes live and exercises the `.repl` parse path; with
      `PX4_RENODE_FIRMWARE` still unset, `tests/smoke.rs` keeps
      skipping until 13.1 produces a firmware artifact.
- [x] **13.8** — Documentation. `tests/renode/README.md` covers
      local setup; `docs/linking-into-px4.md` gains a section
      pointing at the second e2e tier; the roadmap index lists
      phase 13. `docs/research/renode-vs-qemu.md` records the
      substrate decision.

## Acceptance criteria

- [ ] `cd tests/renode && cargo nextest run` boots PX4 on Renode +
      NuttX-H7 and runs the e2e suite to completion without manual
      intervention.
- [ ] At least the equivalents of `boot/`, `smoke/`, `pubsub/`,
      `panic/`, and `multi_wq/` test bodies pass — the same five
      paths the SITL suite covers.
- [ ] Tests are deterministic: 100/100 runs in a row pass without
      flake.
- [ ] CI runtime stays under 5 minutes warm.
- [ ] Without Renode installed (or without the Docker image
      available), the suite reports `[SKIPPED]` rather than
      failing — same shape as `ensure_px4!()` does today.

## Risks

- **PX4 board config is the long pole.** PX4's build system
  expects a board to come with a flotilla of drivers. Stripping
  enough out to fit the "no real I/O" model without breaking the
  build is real work — possibly a week or more of CMake / Kconfig
  surgery. The Nucleo-H743 NuttX port being upstream is a strong
  starting point, but PX4-on-Nucleo isn't.
- **Renode peripheral coverage is moderate.** UART, NVIC, MPU,
  systick, basic timers are solid. Anything fancier (the H7 PWR
  block's lock-step quirks, DMA engines for SPI sensors) needs
  workarounds. None of those should block phase 13's surface,
  but they may force `CONFIG_*` knobs we wouldn't ship in a
  real-board config.
- **Test-time-advance pattern is new.** Renode's `start` /
  `pause` Monitor commands let the host drive virtual time, but
  exposing that from Rust through the existing fixture API
  requires a small protocol on top of the Monitor TCP port.
  Doable, but novel.
- **Boot time may be slow.** A full PX4 stack on virtual H7 with
  Renode's interpretation might take 30–60 s of *virtual* time to
  reach `pxh>`. Mitigate with `pause` after first boot + state
  snapshots that subsequent tests load cheaply.

## Out of scope

- **Sensor / driver e2e.** Renode doesn't model the ICM-20602,
  MS5611, IST8310, etc. on Pixhawk. Phase 13 ships a board with
  no peripherals beyond UART; sensor coverage is HITL or a
  separate `px4-rs-sensor-models` track.
- **Real-time timing assertions.** Renode is deterministic, not
  timing-accurate to physical hardware. Tests assert on ordering
  and counts, not on absolute microsecond budgets.
- **Migration of the SITL POSIX track.** `tests/sitl/` keeps its
  role as the fast pre-merge gate. Phase 13 is additive.
- **HITL.** Real Pixhawk on USB stays a separate, opt-in track —
  documented in `docs/`, not run in CI.

## Notes for implementers

- Reference repos cloned to `external/` (gitignored): `renode/`
  for the stock `.repl` + `.resc` patterns we'll mimic;
  `renode-test-action/` for the GitHub Action shape;
  `renode-example/` for a minimal STM32 + Robot Framework starter.
- The existing `tests/sitl/src/process.rs::graceful_kill` and
  `set_new_process_group` helpers transfer to `tests/renode/`
  unchanged — same Unix process-group cleanup story.
- The phase-12 `Px4Sitl::wait_for_exit` shape is the cleanest
  reference: poll `try_wait` on a `Mutex<Child>` while another
  thread tails the log buffer.
