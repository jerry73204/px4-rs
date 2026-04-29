//! Phase-13.6 port of `tests/sitl/tests/pubsub.rs`.
//!
//! Both halves of this test (`e2e_pubsub_pub` and `e2e_pubsub_sub`)
//! drive their async loops via `yield_now()`, which doesn't yield
//! to the OS scheduler on NuttX — the lp_default WorkQueue thread
//! monopolises the CPU and nsh's prompt never returns. Marking
//! `#[ignore]` until that runtime gap lands; see `e2e_smoke.rs` for
//! the long-form note. Test body is a direct port.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
#[ignore = "yield_now starves nsh on NuttX — see e2e_smoke.rs"]
fn pubsub_round_trip() {
    ensure_renode!();
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!("[SKIPPED] PX4_RENODE_HAS_PX4 not set");
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("e2e_pubsub_sub start")
        .expect("e2e_pubsub_sub start");
    sitl.wait_for_log("e2e_pubsub_sub task started", Duration::from_secs(3))
        .expect("subscriber task didn't start");

    sitl.shell("e2e_pubsub_pub start")
        .expect("e2e_pubsub_pub start");
    sitl.wait_for_log("e2e_pubsub_pub task started", Duration::from_secs(3))
        .expect("publisher task didn't start");

    let line = sitl
        .wait_for_log("got counter=10", Duration::from_secs(5))
        .expect("subscriber never received counter=10 — Subscription path is broken");
    assert!(
        line.contains("got counter=10"),
        "unexpected match line: {line}"
    );
}
