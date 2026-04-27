//! E2E for `examples/multi_task/`. The producer sits on `hp_default`
//! pinging a `Notify` once a second; the consumer sits on
//! `lp_default` waiting on the same `Notify`. We assert that the
//! consumer logs `count=2` — i.e. two cross-WQ wake-ups landed,
//! which only happens if both WQ threads run independently AND the
//! `Notify` permit propagates between them.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn multi_task_consumer_wakes_via_notify() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    sitl.shell("multi_task start").expect("multi_task start");
    sitl.wait_for_log("producer started", Duration::from_secs(2))
        .expect("producer task never reached its banner");
    sitl.wait_for_log("consumer started", Duration::from_secs(2))
        .expect("consumer task never reached its banner");

    // Producer notifies once per second. Two wakeups → ~2s.
    sitl.wait_for_log("consumer woke, count=2", Duration::from_secs(5))
        .expect(
            "consumer didn't observe a second Notify wake — either \
             the producer's WQ isn't running, or Notify is dropping \
             cross-WQ wake-ups",
        );
}
