//! Phase-13.6 port of `tests/sitl/tests/multi_wq.rs`.
//!
//! Direct port of the SITL body. `e2e_multi_wq` runs two `#[task]`s
//! — one on `lp_default`, one on `hp_default` — each printing a
//! banner on first poll and then yielding. We assert both banners
//! land; if only one shows up, `#[task(wq = "...")]` isn't actually
//! routing across queues.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
fn both_workqueues_run_independently() {
    ensure_renode!();
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!("[SKIPPED] PX4_RENODE_HAS_PX4 not set");
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("e2e_multi_wq start").expect("e2e_multi_wq start");

    sitl.wait_for_log("lp_default tick alive", Duration::from_secs(3))
        .expect("lp_default task never reached its banner");
    sitl.wait_for_log("hp_default tick alive", Duration::from_secs(3))
        .expect("hp_default task never reached its banner");
}
