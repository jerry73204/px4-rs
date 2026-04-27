//! E2E for `examples/hello_module/`. Confirms the example actually
//! does what the docstring promises: a single `#[task]` on
//! `lp_default` that wakes once per second via `px4_workqueue::sleep`
//! and prints a numbered banner each tick.
//!
//! The interesting check is that `tick=2` lands — that proves
//! `Sleep` re-arms after the first fire instead of resolving once
//! and getting stuck.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn hello_module_ticks_at_least_twice() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    sitl.shell("hello_module start").expect("hello_module start");
    sitl.wait_for_log("ticker started", Duration::from_secs(2))
        .expect("task body never reached its banner");

    // Two ticks ≈ 2s on a 1Hz timer. Allow generous headroom because
    // SITL's `lockstep_scheduler` can stretch wall time.
    sitl.wait_for_log("hello tick=2", Duration::from_secs(5))
        .expect(
            "Sleep didn't re-arm — second tick never landed in the \
             daemon log",
        );
}
