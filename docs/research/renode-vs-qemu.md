# Renode vs. QEMU + NuttX for Phase-13 SITL-Beyond-POSIX Testing

Pre-decision research for an "actually executes on ARM/NuttX" testing
track. Today's `tests/sitl/` exercises PX4's POSIX build — real
broker, real WorkQueue, but on x86_64 Linux pthreads. The phase-13
goal is a CI-friendly path that runs the same kind of e2e against
the **NuttX + ARM Cortex-M** code path PX4 ships on actual Pixhawks.

Two candidates: **QEMU** and **Renode**. The verdict is Renode; the
reasoning below.

## Decision

**Use Renode.** Three reasons, in order of weight:

1. **NuttX-on-Cortex-M is supported out of the box.** NuttX's official
   Renode guide
   ([apache.org](https://nuttx.apache.org/docs/12.8.0/guides/renode.html))
   lists working ports for `STM32F4Discovery`, `STM32F746G-Disco` and
   `Nucleo-H743ZI`. The H743 is the same SoC family Pixhawk FMU-v6X
   uses (STM32H743VIT6); the F746 maps onto the FMU-v5 era. The Renode
   tree we cloned to `external/renode/` already ships
   `platforms/cpus/stm32h743.repl` and `stm32h753.repl` plus
   `platforms/boards/nucleo_h753zi.repl` — direct hits.
2. **QEMU's Cortex-M coverage is two boards, both TI**, per Memfault's
   Interrupt blog
   ([memfault.com](https://interrupt.memfault.com/blog/intro-to-renode))
   and confirmed by NuttX's own QEMU board configs, which only target
   *Cortex-A* (`qemu-armv7a`) — the application profile, not the
   microcontroller profile Pixhawk runs on. Adding a Cortex-M STM32
   to QEMU is a from-scratch effort; QEMU upstream explicitly is not
   designed with extensibility for embedded targets in mind.
3. **Renode is built for CI.** Antmicro ships a maintained
   `antmicro/renode-test-action@v4` GitHub Action
   ([renode.io](https://renode.io/news/renode-github-action-for-automated-testing-in-simulation/))
   plus a Robot Framework test harness, deterministic virtual time,
   and an official Docker image. ChromeOS uses it in production for
   EC firmware testing
   ([opensource.googleblog.com](https://opensource.googleblog.com/2023/08/chromeos-ec-testing-suite-renode-for-consumer-products/)).
   The CI plumbing is paved road, not yak-shaving.

## Side-by-side

| Dimension | Renode | QEMU |
|---|---|---|
| **License** | MIT | GPL-2.0 |
| **Cortex-M boards supported** | STM32F0/F1/F4/F7/H7, nRF52840, NXP, Nordic, many more | TI Stellaris (LM3S6965, LM3S811) — that's it on the M side |
| **NuttX support** | Documented, working on F4/F7/H7 | Cortex-A only (qemu-armv7a) |
| **Pixhawk SoC family** | STM32H743 + STM32H753 ship as `.repl` files | Not supported |
| **Peripheral fidelity** | High — UART, SPI, I2C, ADC, timers, NVIC, MPU all modelled | Minimal beyond what Linux needs |
| **Multi-device sim** | First-class (whole-system simulator) | Single-system focus |
| **Determinism** | Yes — virtual time, reproducible runs | Approximate |
| **CI integration** | Official GitHub Action, Robot Framework, Docker image | Hand-rolled |
| **Test scripting** | `.resc` (machine setup) + `.robot` (test cases) | None native |
| **Languages of impl** | C# (50%) + Robot + Python | C |
| **Extensibility for new boards** | `.repl` YAML-ish files, new peripheral models in C# | C plumbing in `hw/arm/`; "no stable API, changes version to version" |
| **Production-CI track record** | ChromeOS EC, Zephyr, NuttX, AzureRTOS, FreeRTOS upstream | Linux distros, Android, Yocto |

## Concrete fidelity for our needs

The phase-13 fixture must do four things:

1. **Boot a NuttX + PX4 binary on emulated ARM Cortex-M.** Renode's
   STM32H743 model with NuttX is documented working — the only
   compile-time tweak for NuttX on Renode-H7 is
   `CONFIG_STM32H7_PWR_IGNORE_ACTVOSRDY=y` per the NuttX guide.
2. **Run our `#[task]` / Publication / Subscription / panic_handler
   modules unchanged.** That's a function of the NuttX kernel
   surfacing the same WorkQueue + uORB libs PX4 SITL exposes. The
   Rust runtime is target-agnostic; the difference is the C++/NuttX
   substrate.
3. **Drive `pxh` over UART and read its responses.** Renode
   exposes UART backings as pty / TCP sockets / named pipes; the
   existing `Px4Sitl::wait_for_log` substring matcher transfers
   directly — only the connection method changes from "stdout pipe"
   to "UART pty".
4. **Be deterministic enough to gate merges.** Renode's virtual time
   model means the `airspeed_selector` race we saw in SITL
   (race between PX4's stock subscriber timing and our test poll)
   can't happen under Renode unless we deliberately introduce it.

What QEMU could give us in return: nothing actionable. Even if we
built a custom Cortex-M STM32H7 board for QEMU from scratch, we'd
end up with a peripheral model less complete than Renode's existing
one, no Robot Framework integration, and no determinism guarantees.

## Caveats — known fidelity gaps

Renode is not a silver bullet:

- **No Pixhawk-specific board file ships with Renode.** Closest
  match: `nucleo_h753zi.repl`. FMU-v6X-shape work is creating a
  custom `.repl` that adds the right peripherals (ICM-20602 IMU on
  SPI, etc.) — but for our test surface (uORB, WQ, hrt, log,
  panic) we don't need any sensor model, just the CPU + NVIC +
  UART + RAM. The stock H743 `.repl` covers all of that.
- **Per the NuttX Renode guide, several STM32 peripherals are
  incomplete on Renode**: QSPI, PWM, ADC, I2C touchscreen. None
  of these matter for the runtime tests. They'd matter for a
  hardware-driver e2e, which isn't in scope here.
- **No real-time guarantees.** Renode is a virtual-time simulator;
  it's deterministic but not timing-accurate to physical hardware.
  HRT timing assertions can only check ordering, not absolute
  microsecond budgets. Acceptable — the existing SITL suite has
  the same constraint.
- **Not suitable for HITL-equivalent driver coverage.** Real
  Pixhawk integration testing (i2c sensor on a particular bus,
  actuator output PWM timing, mavlink over UART to a real ground
  station) still needs hardware. Renode is the *middle* tier
  between POSIX SITL and HITL, not a replacement for either.

## What phase 13 looks like (sketch)

```
docs/roadmap/phase-13-renode-nuttx-e2e.md      # phase doc
tests/renode/
    Cargo.toml                                 # standalone, like tests/sitl/
    src/
        fixtures/
            renode.rs                          # Renode lifecycle: spawn `renode --console`, drive Monitor
            uart_pty.rs                        # pty connection, line-tail loop
        lib.rs
    px4-build/                                 # NuttX-on-H743 build of PX4 with our externals tree
        Cargo.toml-shim
    platforms/
        px4_renode_h743.repl                   # extends stock stm32h743.repl with what we need
        px4_renode_h743.resc                   # boots the firmware, opens UART
    tests/
        smoke.robot                            # mirror of tests/sitl/tests/smoke.rs
        ...
```

The Rust-side test bodies are unchanged from `tests/sitl/tests/*.rs` —
the `Px4RenodeSitl` fixture replaces `Px4Sitl::boot()`; everything
downstream (`shell`, `wait_for_log`, `wait_for_exit`) keeps the same
signature.

## What we did so far

Cloned three repos to `external/` (shallow, all under 12 MB combined):

- `external/renode/` — main repo with `.resc` / `.repl` examples,
  including `stm32h743.repl`, `stm32h753.repl`, `nucleo_h753zi.repl`,
  `stm32f7_discovery-bb.repl`. Reference for our own platform file.
- `external/renode-test-action/` — Antmicro's GitHub Action source
  (Dockerfile + entrypoint). Tells us what a `.robot` invocation
  needs.
- `external/renode-example/` — minimal STM32 example with a paired
  Robot Framework test. Closest thing to a phase-13 starter.

Sources:
- [NuttX on Renode — boards and known issues](https://nuttx.apache.org/docs/12.8.0/guides/renode.html)
- [Memfault Interrupt — Cortex-M Emulation with Renode](https://interrupt.memfault.com/blog/intro-to-renode)
- [Memfault Interrupt — Renode + GitHub Actions](https://interrupt.memfault.com/blog/test-automation-renode)
- [Renode supported boards](https://renode.readthedocs.io/en/latest/introduction/supported-boards.html)
- [Renode GitHub Action announcement](https://renode.io/news/renode-github-action-for-automated-testing-in-simulation/)
- [Antmicro renode-test-action repo](https://github.com/antmicro/renode-test-action)
- [ChromeOS EC testing with Renode (Google OSS Blog)](https://opensource.googleblog.com/2023/08/chromeos-ec-testing-suite-renode-for-consumer-products/)
- [NuttX qemu-armv7a — Cortex-A only](https://nuttx.apache.org/docs/latest/platforms/arm/qemu/boards/qemu-armv7a/index.html)
- [Renode main repo](https://github.com/renode/renode)
