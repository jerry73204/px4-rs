//! Phase-13.6 port of `tests/sitl/tests/example_multi_task.rs`.
//!
//! ## Currently `#[ignore]`d
//!
//! The producer's loop hangs off `sleep(Duration::from_secs(1))`,
//! which in turn arms an HRT compare-match. Renode's STM32_Timer
//! model fires the first compare IRQ but not the second one after
//! CCR1 is re-programmed (see `example_hello_module.rs` for the
//! long-form note). Without a second producer wake there's no
//! second `count=2` notify on the consumer side.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
#[ignore = "HRT compare IRQ doesn't re-fire on Renode TIM8 — see example_hello_module.rs"]
fn multi_task_consumer_wakes_via_notify() {
    ensure_renode!();
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!("[SKIPPED] PX4_RENODE_HAS_PX4 not set");
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("multi_task start").expect("multi_task start");
    sitl.wait_for_log("producer started", Duration::from_secs(3))
        .expect("producer task never reached its banner");
    sitl.wait_for_log("consumer started", Duration::from_secs(3))
        .expect("consumer task never reached its banner");

    sitl.wait_for_log("consumer woke, count=2", Duration::from_secs(6))
        .expect(
            "consumer didn't observe a second Notify wake — either \
             the producer's WQ isn't running, or Notify is dropping \
             cross-WQ wake-ups",
        );
}
