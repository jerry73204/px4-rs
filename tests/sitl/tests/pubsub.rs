//! Phase-11 work item 11.7 — `Subscription` round-trip through PX4's
//! real broker.
//!
//! Two modules participate: `e2e_pubsub_pub` publishes incrementing
//! `E2ePubsub` samples, and `e2e_pubsub_sub` subscribes and logs a
//! `got counter=N` line for every sample it receives. The test boots
//! one daemon with both modules linked in, starts the subscriber
//! first, then the publisher, and waits for a high-enough counter to
//! prove that messages actually flowed.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn pubsub_round_trip() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    // Subscriber first, so the broker has a registered callback by
    // the time the publisher's first sample lands. Order doesn't
    // strictly matter for correctness — `Subscription::recv` would
    // catch up on whatever's in the queue — but starting the sub
    // first removes a race where the very first publish gets dropped
    // before any subscriber is wired up.
    sitl.shell("e2e_pubsub_sub start")
        .expect("e2e_pubsub_sub start");
    sitl.wait_for_log("e2e_pubsub_sub task started", Duration::from_secs(2))
        .expect("subscriber task didn't start");

    sitl.shell("e2e_pubsub_pub start")
        .expect("e2e_pubsub_pub start");
    sitl.wait_for_log("e2e_pubsub_pub task started", Duration::from_secs(2))
        .expect("publisher task didn't start");

    // Counter starts at 1 and increments every yield. By the time
    // the broker has dispatched 10 wakes to the subscriber, we know
    // both halves are alive and the Subscription path is delivering.
    let line = sitl
        .wait_for_log("got counter=10", Duration::from_secs(5))
        .expect("subscriber never received counter=10 — Subscription path is broken");
    assert!(
        line.contains("got counter=10"),
        "unexpected match line: {line}"
    );
}
