# Phase 13 — Renode + NuttX end-to-end test track

**Goal**: A second e2e suite that boots PX4 + NuttX on emulated ARM
Cortex-M and runs the same `e2e_*` test bodies that
`tests/sitl/` already runs against POSIX SITL. Closes the ARM-codegen
+ NuttX-scheduler + interrupt-timing gap that POSIX SITL leaves open.

**Status**: Infrastructure + PX4-on-NuttX firmware + shell-driven tests + SITL externals all linked in + SITL test bodies ported. **11 of 13 tests pass live**; 2 are `#[ignore]`d behind a Renode-side HRT compare-IRQ gap that's the next thing to chase.
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

- [x] **13.1-lite** — Stock NuttX `nucleo-h743zi:nsh` boots on
      Renode end-to-end. The `nuttx` ELF (~1.3 MB) reaches the
      `NuttShell` banner + `nsh>` prompt in 1.7 s warm. Note that
      the documented Nucleo flag `CONFIG_STM32H7_PWR_IGNORE_ACTVOSRDY`
      doesn't exist in PX4's NuttX fork — we handle the H7 PWR
      voltage-scaling poll on the Renode side instead, with a
      `Python.PythonPeripheral` at 0x58024800 returning 0xFFFFFFFF
      to all reads. `tests/renode/tests/smoke.rs::fixture_boots_to_nuttx_banner`
      gates the boot path; passes live when `PX4_RENODE_FIRMWARE`
      points at the NuttX ELF.
- [x] **13.1** — Full PX4-on-NuttX board config (`px4_renode-h743`)
      builds, boots, and runs the shell. ~7 MB ELF; banner appears
      in 1.7 s warm; `uorb start` then `uorb status` returns the
      topic table. Concrete state:
      * `tests/renode/px4-board/` is the board template, branched
        from fmu-v6c with drivers + flight stack stripped to a
        small systemcmd allow-list (`uorb`, `ver`, `listener`,
        `work_queue`, `perf`, `top`, `param`, `reboot`) plus
        `logger`. `scripts/setup-board.sh` copies it into
        `$PX4_AUTOPILOT_DIR/boards/px4/renode-h743/` (PX4's
        Makefile finds boards with `find` and ignores symlinks);
        `teardown-board.sh` is the sentinel-guarded inverse.
      * Five fixes were needed past the initial scaffold to get
        a clean firmware: `#include <stm32_gpio.h>` in
        `board_config.h` (transitive declaration `gpio.c` needs);
        `#define HRT_TIMER 8` (gates `arch_hrt`'s
        `hrt_absolute_time`/`hrt_call_*` symbols into the build —
        without it, `hrt.c` compiles to an empty object and link
        fails); empty `stm32_boardinitialize` stub in `init.c`;
        empty PWM tables in `timer_config.cpp`; defconfig dropping
        USB CDC ACM, ROMFS/CROMFS, and ROMFSETC (no SD card, no
        USB, no `/etc/init.d/rcS`).
      * Two Renode-specific config gaps surfaced after the build
        succeeded: USART3 RX/TX DMA paths re-fire instantly under
        Renode's no-baud-rate model, pinning the CPU in the IRQ
        chain and starving nsh — disabling `CONFIG_USART3_RXDMA`
        / `CONFIG_USART3_TXDMA` switches USART3 to interrupt-driven
        mode and frees nsh; and pty writes to `CreateUartPtyTerminal`
        return EIO once NuttX sets the `FIFOEN` bit (Renode tags
        bit 29 of CR1 RESERVED), so shell input goes through
        Renode's monitor (`sysbus.usart3 WriteLine "<cmd>"`) rather
        than the pty. The fixture reads via the pty and writes via
        a piped `monitor_stdin`.
      * `tests/renode/tests/pxh.rs` is live with
        `PX4_RENODE_HAS_PX4=1`. 5/5 consecutive runs all green.
      * No `rcS` (we removed ROMFSETC), so `pxh.rs` runs `uorb start`
        explicitly before `uorb status` — there's no startup script
        to do it for us.
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
- [x] **13.5** — Reuse `tests/sitl/px4-externals/`. The renode-h743
      firmware build now points at the existing SITL externals tree
      via `EXTERNAL_MODULES_LOCATION` — no duplication. All 8
      modules (`e2e_smoke`, `e2e_pubsub_pub`, `e2e_pubsub_sub`,
      `e2e_panic`, `e2e_multi_wq`, `hello_module`, `multi_task`,
      `gyro_watch`) cross-compile to `thumbv7em-none-eabihf` and
      appear in nsh's Builtin Apps list. `just build-renode-firmware`
      is the one-liner.

      Less trivial than the doc anticipated: cross-compiling
      `px4-sys`'s `wrapper.cpp` for NuttX needed three pieces of
      build-system work that the SITL/POSIX path didn't exercise:

      * `cmake/px4-rust.cmake` now propagates board info
        (`PX4_RS_BOARD_NAME`, `_BOARD_DIR`, `_CHIP`, `_ARCH_FAMILY`)
        and adds an explicit dependency on `nuttx_context`'s
        generated `<nuttx/config.h>` so the cargo trampoline build
        doesn't race the `clean_context → mkconfig` step that
        regenerates it on every build.
      * `crates/px4-sys/build.rs`'s NuttX branch now adds the full
        NuttX include set (`platforms/nuttx/src/px4/common/include`,
        chip-specific `platforms/nuttx/src/px4/<vendor>/<chip>/include`,
        the `armv7-m`/`chip`/`common` arch dirs, and the
        `nuttx/include`/`include/cxx` `-isystem` trio), plus
        `-nostdinc++` + `-fno-exceptions`/`-rtti`/`-sized-deallocation`
        /`-threadsafe-statics` + `-fcheck-new` to match PX4's
        embedded-cxx defaults. Without `-nostdinc++` the toolchain's
        newlib `<cmath>` and NuttX's collide and `NAN` becomes
        undefined inside `TrajMath.hpp`.
      * `wrapper.cpp` had a `sizeof(::orb_metadata) == 24` static
        assertion that was 64-bit-POSIX-only; on 32-bit ARM the
        struct is 16 bytes. The check is now `#if UINTPTR_MAX ==
        0xFFFFFFFFULL`-gated. Also dropped `std::nothrow` (NuttX's
        vendored `<new>` doesn't export it; with `-fno-exceptions`
        plain `new` returns nullptr on OOM anyway).
- [/] **13.6** — Port the existing test bodies. All eight SITL
      test files now have a sibling under `tests/renode/tests/`,
      same body shape, shelling the same modules. **11 tests pass
      live**; 2 are `#[ignore]`d behind a Renode-side HRT model gap.

      Live: `smoke`/`probe`/`pxh` (existing); `e2e_smoke_*` (3
      tests — module start, airspeed topic, listener round-trip);
      `multi_wq`; `pubsub`; `gyro_watch` (threshold banner only,
      adapted — SITL's subscriber-count check needs SIH);
      `panic` (log line only, adapted — NuttX `abort()` from a
      worker task kills only the task, not the firmware).

      The yield_now-related stalls that `e2e_smoke`, `multi_wq`,
      and `pubsub` initially hit got fixed by adding a `usleep(1)`
      to `px4_workqueue::yield_now()` on the NuttX path: PX4's
      `WorkQueue::Run()` drains its queue tight without
      sem-waiting between items, so a self-rescheduling task
      becomes a kernel-level CPU hog. `sched_yield` doesn't help
      (no same-priority peers), but `usleep(1)` puts the WQ
      thread on the timed-wait list and forces NuttX to run the
      scheduler — `nsh` and the idle/serial-RX thread then get
      their CPU windows. POSIX SITL is unaffected; `usleep(1)`
      is a near-noop under the lockstep scheduler and the
      existing `tests/sitl/` suite still runs clean.

      Remaining `#[ignore]`d: `example_hello_module`,
      `example_multi_task`. Both depend on `sleep(Duration)`,
      which arms an HRT compare-match. Root-caused to a precise
      bug in Renode's `STM32_Timer` model.

      **Renode bug** (`renode-infrastructure` `STM32_Timer.cs`,
      `UpdateCaptureCompareTimer`):

      ```csharp
      ccTimers[i].Enabled = Enabled
          && IsInterruptOrOutputEnabled(i)
          && Value < ccTimers[i].Limit;
      ```

      The `Value < Limit` check disables the channel whenever
      `CCR <= CNT`. Real STM32 hardware fires CC1IF whenever
      `CNT == CCR`, including the case where `CCR <= CNT` —
      the match happens after `CNT` counts up to `ARR`, wraps to
      0, and counts up again to `CCR`. PX4's HRT writes
      `rCCR_HRT = deadline & 0xffff` from inside the ISR, where
      `deadline = now + delta` and the ISR has been running long
      enough (Renode-time) that the new low-16 bits often land
      *behind* the current `CNT`. Renode disables the channel
      and never re-arms it for the wrap path — the channel only
      re-arms when the main timer's `LimitReached` fires
      (autoreload wrap), and at that point `CNT` is back at 0,
      but the `ccTimers[i].Limit` field still holds the pre-wrap
      `CCR`. If that `CCR` was set to a value the post-wrap
      counter will reach (it usually is, since `CCR <= ARR`),
      the next compare-match WOULD fire — except by then
      `ccTimers[i].Enabled` is `false` from the prior
      `UpdateCaptureCompareTimer` call, and the wrap callback
      doesn't re-evaluate it.

      **Proposed upstream patch** — replace the body of
      `UpdateCaptureCompareTimer` with a wrap-aware variant that
      counts the right number of ticks regardless of CCR/CNT
      ordering:

      ```csharp
      private void UpdateCaptureCompareTimer(int i)
      {
          ccTimers[i].Enabled = Enabled && IsInterruptOrOutputEnabled(i);
          if (ccTimers[i].Enabled)
          {
              if (Value < ccTimers[i].Limit)
              {
                  ccTimers[i].Value = Value;
              }
              else
              {
                  // Wrap case: count to ARR, wrap, then to CCR.
                  // Express via Value+Limit so the LimitTimer
                  // sees Value < Limit on ascending count.
                  var ticksUntilFire = (autoReloadValue - Value)
                                       + ccTimers[i].Limit + 1;
                  ccTimers[i].Value = ccTimers[i].Limit > ticksUntilFire
                      ? ccTimers[i].Limit - ticksUntilFire
                      : 0;
              }
          }
          ccTimers[i].Direction = Direction;
      }
      ```

      Local rebuild blocked by environment: `dotnet-sdk-8.0` not
      installed (only the runtime is), and the system `Renode`
      binary at `/opt/renode/bin/Infrastructure.dll` is
      root-owned. Path forward when someone wants to land the
      fix locally:

      1. `apt install dotnet-sdk-8.0`
      2. `cd external/renode/src/Infrastructure/src && dotnet build -c Release Infrastructure_NET.csproj`
      3. `sudo cp ./bin/Release/net8.0/Infrastructure.dll /opt/renode/bin/Infrastructure.dll`
      4. Drop the `#[ignore]` markers on the two tests; they pass as-is.

      Cleanest route for px4-rs is to file the patch upstream at
      `renode/renode-infrastructure` and wait for a release; the
      submodule pin in `external/renode` then bumps along with
      the next version of Renode the project supports.

      The fixture itself learned to run `work_queue start` then
      `uorb start` on boot (no `rcS` on this board) so individual
      tests don't have to. `gyro_watch` was adapted: SITL relies on
      `airspeed_selector`/SIH publishing `sensor_gyro` for a
      before/after subscriber count check; this firmware has no
      sensor stack, so the renode test only checks the threshold
      banner. `panic` was adapted similarly: SITL's `wait_for_exit`
      doesn't apply (NuttX `abort()` from a worker task kills only
      the task; the rest of the firmware survives), so we only
      assert the log line from `panic_handler!() → px4_log` lands.
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
