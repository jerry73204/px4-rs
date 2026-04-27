//! Phase-11 work item 11.9 — multiple WorkQueues running concurrently.
//!
//! `e2e_multi_wq` spawns one `#[task]` on `lp_default` and one on
//! `hp_default`. Each task logs a banner on first poll. The test
//! starts the module and waits for both banners; if only one
//! appears, the runtime is pinning everything to a single WQ and
//! `#[task(wq = "...")]` isn't actually routing.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn both_workqueues_run_independently() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    sitl.shell("e2e_multi_wq start").expect("e2e_multi_wq start");

    sitl.wait_for_log("lp_default tick alive", Duration::from_secs(3))
        .expect("lp_default task never reached its banner");
    sitl.wait_for_log("hp_default tick alive", Duration::from_secs(3))
        .expect("hp_default task never reached its banner");
}
