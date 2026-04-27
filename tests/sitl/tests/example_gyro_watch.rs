//! E2E for `examples/gyro_watch/`. Two checks:
//!
//! 1. The watcher actually starts — its banner mentions the
//!    threshold value, which is the kind of free-text breadcrumb
//!    that goes silently missing if the `info!` macro or `module!`
//!    plumbing regresses.
//! 2. The watcher subscribes to `sensor_gyro` — verified by reading
//!    `uorb status` and confirming the subscriber count went up by
//!    at least one after the watcher started. SITL's SIH simulator
//!    publishes sensor_gyro at 250 Hz, so the topic always exists;
//!    the question is whether our `Subscription` registered.
//!
//! The spike-detection path (Publication of `gyro_alert`) needs a
//! deliberate stimulus and isn't exercised here — SITL's stationary
//! sim values stay well under the 2.5 rad/s threshold. Adding a
//! companion stimulus module is straightforward but out of scope
//! for the bring-up smoke test.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn gyro_watch_subscribes_to_sensor_gyro() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    // Snapshot the subscriber count before starting the watcher so
    // we can detect the increment regardless of how many SITL
    // modules were already subscribed at boot.
    let before = sensor_gyro_sub_count(&sitl);

    sitl.shell("gyro_watch start").expect("gyro_watch start");
    sitl.wait_for_log("watcher started, threshold=2.5 rad/s", Duration::from_secs(2))
        .expect("watcher task never logged its threshold banner");

    // Give the broker a moment to register the new subscriber. The
    // `Subscription::recv()` call is what triggers
    // `sub_cb_register` lazily, so the count jump only happens
    // after the task is polled at least once.
    std::thread::sleep(Duration::from_millis(200));

    let after = sensor_gyro_sub_count(&sitl);
    assert!(
        after > before,
        "sensor_gyro #SUB didn't increase after gyro_watch start \
         (before={before}, after={after}) — Subscription::recv() \
         never registered the callback",
    );
}

/// Read `uorb status`, find the `sensor_gyro` row, and return its
/// `#SUB` column (subscriber count).
fn sensor_gyro_sub_count(sitl: &Px4Sitl) -> u32 {
    let status = sitl.shell("uorb status").expect("uorb status");
    let line = status
        .lines()
        .find(|l| l.starts_with("sensor_gyro "))
        .unwrap_or_else(|| panic!("no `sensor_gyro` row in uorb status:\n{status}"));
    let cols: Vec<&str> = line.split_whitespace().collect();
    // Columns: TOPIC INST #SUB #Q SIZE PATH
    cols.get(2).and_then(|s| s.parse().ok()).unwrap_or(0)
}
