# Phase 11 — SITL end-to-end test infrastructure

**Goal**: A `cargo test` (more precisely `cargo nextest run` against a
dedicated test crate) that boots PX4 SITL with Rust modules linked in,
drives the live `px4` daemon, and asserts on what comes back through
uORB. Catches the entire integration stack — codegen, cargo build,
wrapper.cpp compile, PX4 link, runtime — in one place.

**Status**: Not Started
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
- [ ] 11.2 — `Px4Sitl` fixture: `boot()` (cached `make px4_sitl`,
      spawn `./bin/px4 -d etc/init.d-posix/rcS`, wait for `Startup
      script returned`), `shell(cmd)`, RAII `Drop` (SIGTERM → wait →
      SIGKILL with 5s grace)
- [ ] 11.3 — `wait_for_log(pattern, timeout)` helper that streams the
      daemon's stderr through a `BufReader`
- [ ] 11.4 — `skip!` macro + `PX4_AUTOPILOT_DIR` precondition check
      so the suite degrades gracefully when run without PX4
- [ ] 11.5 — `px4-externals/` tree with `e2e_smoke` module (smallest
      possible: spawns one task that publishes on `airspeed`)
- [ ] 11.6 — First test (`tests/smoke.rs::heartbeat_publishes`)
      reproducing the manual SITL bring-up we did
- [ ] 11.7 — `e2e_pubsub_pub` + `e2e_pubsub_sub` modules + a test
      that verifies they exchange data (covers Subscription path)
- [ ] 11.8 — `e2e_panic` module + a test that the daemon logs
      `[heartbeat] panic` and exits non-zero when commanded to panic
- [ ] 11.9 — `e2e_multi_wq` module with two tasks on different WQs +
      a test that both run independently
- [ ] 11.10 — `just test-sitl` recipe + section in
      `docs/linking-into-px4.md` pointing at the test crate as the
      canonical "does this still work?" answer

## Acceptance criteria

- [ ] `cd tests/sitl && cargo nextest run` passes locally with
      `PX4_AUTOPILOT_DIR=~/repos/PX4-Autopilot` set
- [ ] Without `PX4_AUTOPILOT_DIR`, every test reports `[SKIPPED] PX4
      tree not configured` rather than failing
- [ ] First-time run takes < 60s (PX4 build); subsequent runs < 30s
      (boot × N tests, no rebuild)
- [ ] At minimum one test exercises each of: `#[task]` spawn,
      `Publication`, `Subscription`, `panic_handler!()`,
      multi-WorkQueue scheduling
- [ ] CI workflow stub committed (untested without a runner that has
      PX4 + arm/posix toolchain — but documents the invocation)

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
