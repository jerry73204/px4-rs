//! Phase-13.6 port of `tests/sitl/tests/example_gyro_watch.rs`,
//! adapted for the renode-h743 firmware. SITL's POSIX simulator
//! publishes `sensor_gyro` at 250 Hz; this firmware doesn't run
//! a sensor stack, so we can't verify the spike-detection or
//! subscriber-count-increase paths. What's still exercisable:
//!
//!   - The watcher's main entry point links and runs on Cortex-M7.
//!   - The threshold banner from the task body lands, proving the
//!     task spawned via PX4's WorkQueue manager.
//!
//! Subscription registration is hard to verify here because, with
//! no publisher, the broker may not register the topic at all
//! before our `recv()` call; the SITL test relies on
//! `airspeed_selector` already being subscribed at boot for the
//! before/after delta.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
fn gyro_watch_starts_and_logs_threshold() {
    ensure_renode!();
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!("[SKIPPED] PX4_RENODE_HAS_PX4 not set");
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("gyro_watch start").expect("gyro_watch start");
    sitl.wait_for_log("watcher started, threshold=2.5 rad/s", Duration::from_secs(3))
        .expect("watcher task never logged its threshold banner");
}
