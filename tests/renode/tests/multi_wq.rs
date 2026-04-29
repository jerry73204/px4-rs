//! Phase-13.6 port of `tests/sitl/tests/multi_wq.rs`.
//!
//! See the file-level note in `e2e_smoke.rs` — the same `yield_now()`
//! starvation hits this module too. Marking `#[ignore]` until the
//! `px4-workqueue` runtime gap lands. Test body is a direct port so
//! the fix can flip the marker.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
#[ignore = "yield_now starves nsh on NuttX — see e2e_smoke.rs"]
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
