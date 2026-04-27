# Phase 11 — SITL end-to-end test infrastructure

**Goal**: A `cargo test` (more precisely `cargo nextest run` against a
dedicated test crate) that boots PX4 SITL with Rust modules linked in,
drives the live `px4` daemon, and asserts on what comes back through
uORB. Catches the entire integration stack — codegen, cargo build,
wrapper.cpp compile, PX4 link, runtime — in one place.

**Status**: Done — 9 tests pass under `cargo nextest run` in ~27s warm.
**Priority**: P0 (every later phase change risks silently breaking the
SITL bring-up that we hand-validated end-to-end)
**Depends on**: Phase 02, Phase 04, Phase 06, Phase 07

**Reference**: this design borrows wholesale from
`~/repos/nano-ros/packages/testing/nros-tests/`. RAII fixtures, an
out-of-workspace test crate, `skip!` macro for missing prereqs, and a
nextest test-group for serial execution of resource-conflicting tests.

## Why a separate phase

Phase 09 (host-side mocks) is substantively done — the `std` features
on `px4-workqueue` and `px4-uorb` already let unit tests run without a
PX4 link target, and the round-trip test in phase 06 exercises that
path. What phase 09 doesn't cover, and what one round of manual SITL
bring-up surfaced eight separate integration bugs in, is the
**real PX4 link path**: cargo + cc + PX4 CMake + the C++ trampolines
+ uORB's actual broker. That's a different runtime and warrants its
own test infra.

## Design — three decisions

### Decision 1 — Per-test daemon, not shared

Each `#[test]` boots its own `px4` subprocess via the `Px4Sitl`
fixture, which kills it cleanly on `Drop`. Costs ~3 seconds startup
per test; for a 10-test suite that's ~30 seconds total — acceptable
given how rare full SITL regression cycles are.

The shared-daemon alternative would force tests to clean up uORB
subscriber state between runs and would tie test ordering to the
daemon's notion of advertised topics. Per-test daemons are isolated
by construction.

### Decision 2 — One PX4 binary, all test modules linked in

We `make px4_sitl EXTERNAL_MODULES_LOCATION=tests/sitl/px4-externals`
**once per test session** via a `OnceLock`-guarded fixture. The
external-modules tree contains every test module, all linked into
the same `px4` binary. Each individual `#[test]` then starts only
the modules it cares about (e.g. `./bin/px4-e2e_pubsub start`).

Rebuilding PX4 per test is too slow (~60s cold). The flexibility loss
from baking all modules into one binary is irrelevant — you only need
one binary that contains a superset of what any test needs.

### Decision 3 — Test crate lives outside the main workspace

`tests/sitl/Cargo.toml` is its own workspace, not a member of
`/Cargo.toml`. Three reasons:

  - The test crate uses `std`, `tokio`-style async, `tempfile`,
    `duct`, etc. — none of which the main `no_std` workspace wants
    transitively pulled in.
  - The PX4 modules under `tests/sitl/px4-externals/src/modules/`
    install `panic_handler!()` in their crate root; making them
    workspace members reintroduces the duplicate-lang-item conflict
    we already escaped from in `examples/`.
  - Matches the
    `~/repos/nano-ros/packages/testing/nros-tests/` layout, which is
    the closest precedent in the embedded-Rust + native-broker space.

## Layout

```
tests/sitl/
    Cargo.toml                            # standalone workspace, not a main-workspace member
    rust-toolchain.toml                   # nightly pin (TAIT)
    .config/nextest.toml                  # serial test-group for SITL tests
    src/
        lib.rs                            # re-exports + skip! macro
        fixtures/
            mod.rs
            build.rs                      # OnceLock-cached `make px4_sitl`
            px4_sitl.rs                   # Px4Sitl: boot, shell, drop
        process.rs                        # graceful-kill helper, port helpers
        wait.rs                           # wait_for_pattern on subprocess output
    px4-externals/                        # the PX4-shaped tree
        src/
            CMakeLists.txt                # config_module_list_external = [...]
            modules/
                e2e_smoke/                # one PX4 module per scenario
                    Cargo.toml
                    CMakeLists.txt
                    Kconfig
                    src/lib.rs
                    rust-toolchain.toml
                e2e_pubsub_pub/
                e2e_pubsub_sub/
                e2e_panic/
                e2e_multi_wq/
    tests/
        smoke.rs                          # cargo test --test smoke
        pubsub.rs
        panic.rs
        multi_wq.rs
```

## Public API of the test crate

```rust
use px4_sitl_tests::{Px4Sitl, skip};
use rstest::rstest;

#[rstest]
fn heartbeat_publishes() -> px4_sitl_tests::Result<()> {
    let sitl = Px4Sitl::boot()?;
    sitl.shell("e2e_smoke start")?;
    sitl.wait_for_log("e2e_smoke task started", Duration::from_secs(2))?;

    let listener_out = sitl.shell("listener airspeed")?;
    assert!(listener_out.contains("indicated_airspeed_m_s"));
    Ok(())
}
```

`Px4Sitl::shell(cmd)` execs `./bin/px4-<first-word>` against the
running daemon and returns captured stdout. `wait_for_log(pat, dur)`
tails the daemon's stderr until a regex hits or timeout.

## Work items

- [x] 11.1 — `tests/sitl/` workspace skeleton: standalone `Cargo.toml`,
      `rust-toolchain.toml`, `.config/nextest.toml` with
      `[test-groups] sitl = { max-threads = 1 }`. Skeleton lib defines
      `TestError` / `Result` / `skip!`. `just test-sitl` recipe.
      Excluded from the main workspace via `[workspace] exclude`.
- [x] 11.2 — `Px4Sitl` fixture: `boot()` runs cached `make px4_sitl`
      (`OnceLock`), spawns `./bin/px4 -d etc/init.d-posix/rcS` in its
      own process group, drains stdout+stderr into a shared
      `Mutex<String>` log buffer, blocks on a `Condvar` until the
      `Startup script returned successfully` line appears. `shell(cmd)`
      execs `./bin/px4-<modname>` with the rest as args. `Drop` SIGTERMs
      the process group, waits 3s, then SIGKILLs. Three smoke tests in
      `tests/boot.rs` cover the boot + shell + wait_for_log paths and
      pass against PX4 v1.16.2 SITL in <1s each (warm cache).
- [ ] 11.3 — `wait_for_log` regex upgrade. Substring version is in
      `Px4Sitl::wait_for_log` already; the regex form is a follow-up
      once a test actually needs pattern groups.
- [x] 11.4 — `skip!` macro emits `[SKIPPED] <reason>` to stderr and
      returns; `ensure_px4!()` shorthand checks `PX4_AUTOPILOT_DIR`
      and short-circuits the test. Verified: `cargo nextest run`
      without the env var reports 3 PASS in 5ms with skip lines on
      stderr.
- [x] 11.5 — `px4-externals/src/modules/e2e_smoke/` ships a minimal
      Rust PX4 module: one `#[task]` on `lp_default` that publishes
      Airspeed in a tight loop with `yield_now`. Wired into
      `config_module_list_external`. Standalone `cargo build` and
      full `make px4_sitl` both succeed; the resulting daemon
      registers `airspeed` (2 subscribers — airspeed_selector +
      airspeed_validated pick it up) when `e2e_smoke start` is run.
- [x] 11.6 — `tests/smoke.rs` reproduces the manual SITL bring-up
      across three tests: `e2e_smoke_starts_and_logs` waits for the
      `#[task]` body's banner in the daemon log;
      `airspeed_topic_appears_in_uorb_status` parses the `uorb status`
      table and asserts ≥1 subscriber on `airspeed`;
      `listener_airspeed_reads_back_rust_publish` asserts PX4's stock
      `listener` tool reads the Rust-published payload through the
      canonical-orb_metadata path (`confidence: 1.00000` round-trips).
      Full SITL suite (6 tests) runs in ~12s warm.
- [x] 11.7 — `e2e_pubsub_pub` + `e2e_pubsub_sub` modules + a test
      that verifies they exchange data (covers Subscription path).
      The two staticlibs each carry their own per-crate
      `__ORB_META_E2E_PUBSUB` static; PX4's broker rejects two
      different metadata pointers for the same topic name, so the
      test crate ships an external `msg/E2ePubsub.msg` (+ a tiny
      `msg/CMakeLists.txt` setting `config_msg_list_external`).
      That makes PX4 codegen canonical metadata; both crates'
      `metadata()` then resolve through `px4_rs_find_orb_meta` to
      the same pointer and the broker is happy. Confirms the
      `Subscription` path delivers (test asserts a `got counter=10`
      log line from the subscriber after the publisher starts).
- [x] 11.8 — `e2e_panic` module + a test (`tests/panic.rs`) that
      asserts the panic body lands in the daemon log via
      `panic_handler!()` and the daemon exits non-zero. Required a
      new `Px4Sitl::wait_for_exit(timeout)` helper since every other
      test relies on `Drop` to kill the daemon. Covers the
      `panic_handler!()` install path end-to-end.
- [x] 11.9 — `e2e_multi_wq` module with one `#[task]` on
      `lp_default` and one on `hp_default`. `tests/multi_wq.rs`
      asserts both tasks reach their banner; if `#[task(wq = …)]`
      were silently routing everything to a single WQ thread, only
      one would log.
- [x] 11.10 — `just test-sitl` recipe wired in the top-level
      justfile, plus an "End-to-end regression suite" section in
      `docs/linking-into-px4.md` pointing at `tests/sitl/` as the
      canonical "does this still work?" answer.

## Acceptance criteria

- [x] `cd tests/sitl && cargo nextest run` passes locally with
      `PX4_AUTOPILOT_DIR=~/repos/PX4-Autopilot` set
- [x] Without `PX4_AUTOPILOT_DIR`, every test reports `[SKIPPED] PX4
      tree not configured` rather than failing
- [x] First-time run takes < 60s (PX4 build); subsequent runs < 30s
      (9 tests in ~27s warm)
- [x] At minimum one test exercises each of: `#[task]` spawn
      (smoke), `Publication` (smoke), `Subscription` (pubsub),
      `panic_handler!()` (panic), multi-WorkQueue scheduling
      (multi_wq)
- [x] CI workflow committed at `.github/workflows/ci.yml`. SITL is
      not driven from CI (no PX4 build env on the GitHub runners by
      default), but the `cross-build` matrix covers the
      thumbv7em/thumbv8m/riscv32imc target builds and the
      `px4-sys-snapshot` job gates the bindings against PX4 v1.16.2.

## Out of scope

- Hardware-in-the-loop tests against a real Pixhawk
- Multi-vehicle SITL or simulation-physics correctness
- Performance / latency benchmarking (separate phase if needed)
- ROS 2 interop tests (would belong to nano-ros, not here)

## Risks

- **PX4 SITL changes between versions** — `make px4_sitl` output
  layout could shift (e.g., `bin/` → `usr/bin/`). The fixture must
  derive the binary path from the build dir, not hardcode it.
- **uORB orphans** — if a test's `Drop` doesn't fire (SIGKILL of the
  test runner), the px4 daemon survives and locks ports for the next
  run. Adopt the nros-tests `kill_listeners_on_port` pattern as a
  pre-boot cleanup.
- **Rust + cc parallel compilation** under nextest — multiple test
  processes might race on the shared `CARGO_TARGET_DIR`. The
  PX4-build fixture must hold a `OnceLock` AND a file lock so the
  build runs once even across nextest's separate processes.

## Open questions

- Should the test crate's modules be regenerated from a template at
  build time, or hand-maintained? Hand-maintained for the first
  cut — the templates need updates anyway when phase changes touch
  the macro contract, and a generator adds complexity.
- nextest vs. plain `cargo test`? nextest, because the per-process
  isolation is what guarantees `Drop`-based cleanup runs even if
  one test panics.
