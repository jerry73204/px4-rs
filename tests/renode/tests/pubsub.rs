//! Phase-13.6 port of `tests/sitl/tests/pubsub.rs`.
//!
//! Direct port of the SITL body. `e2e_pubsub_pub` publishes
//! incrementing samples; `e2e_pubsub_sub` subscribes and logs a
//! `got counter=N` line for every sample it receives. The test starts
//! the subscriber first (so the broker has a callback registered when
//! the publisher's first sample lands), then the publisher, and waits
//! for `counter=10` — proof the round-trip through PX4's broker
//! actually delivers messages.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
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
