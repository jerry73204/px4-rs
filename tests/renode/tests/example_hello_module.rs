//! Phase-13.6 port of `tests/sitl/tests/example_hello_module.rs`.
//!
//! ## Currently `#[ignore]`d
//!
//! `examples/hello_module/` calls `sleep(Duration::from_secs(1))`
//! between ticks. PX4's HRT driver on STM32H7 uses TIM8 channel 1
//! in compare mode: ISR fires when CCR1 matches CNT, the callback
//! runs queued `hrt_call`s, then the next deadline is programmed
//! into CCR1. Empirically Renode's STM32_Timer model fires the
//! first compare-match interrupt but doesn't fire a second one
//! after CCR1 is reprogrammed — so `tick=1` lands and nothing
//! after. The same gap stalls anything that arms an HRT-backed
//! one-shot more than once: `Sleep` re-arms, `ScheduleDelayed`,
//! `ScheduleOnInterval`. The `gyro_watch` and `panic` tests pass
//! because they don't touch HRT (Subscription wakes via uORB
//! callback; panic fires on first poll).
//!
//! Tracking the Renode-model fix separately. Once HRT compare
//! interrupts re-fire, drop the `#[ignore]` and this test should
//! pass with the existing body.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
#[ignore = "HRT compare IRQ doesn't re-fire on Renode TIM8 — see file-level docs"]
fn hello_module_ticks_at_least_twice() {
    ensure_renode!();
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!("[SKIPPED] PX4_RENODE_HAS_PX4 not set");
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("hello_module start").expect("hello_module start");
    sitl.wait_for_log("ticker started", Duration::from_secs(3))
        .expect("task body never reached its banner");

    // Two ticks ≈ 2 s of virtual time; Renode runs ~real-time on
    // a fast host so the wall-clock budget is similar to SITL's.
    sitl.wait_for_log("hello tick=2", Duration::from_secs(6))
        .expect(
            "Sleep didn't re-arm — second tick never landed in the \
             firmware log",
        );
}
