//! Reproduces the manual SITL bring-up done during phase-07 in code:
//! boot a fresh `px4` daemon, start the `e2e_smoke` Rust module,
//! verify the publication makes it into uORB, and verify PX4's
//! built-in `listener` tool reads it back through the canonical
//! metadata path.
//!
//! This is the first test that actually exercises the
//! `cargo + cc + PX4 link + uORB broker` chain end to end.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn e2e_smoke_starts_and_logs() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    let out = sitl.shell("e2e_smoke start").expect("e2e_smoke start");
    // The module's main() prints "started" on its info channel; the
    // shell client also surfaces that line.
    assert!(
        out.contains("started"),
        "expected `e2e_smoke start` to log 'started', got:\n{out}"
    );

    // The #[task] body prints its own banner once it's first polled
    // by the WorkQueue. That happens shortly after spawn — wait for
    // it via the streaming log buffer so we know the task ran at all,
    // not just that the entry point returned.
    let task_banner = sitl
        .wait_for_log("e2e_smoke task started", Duration::from_secs(2))
        .expect("task body never reached its banner — #[task] didn't run");
    assert!(task_banner.contains("e2e_smoke task started"));
}

#[test]
fn airspeed_topic_appears_in_uorb_status() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");
    sitl.shell("e2e_smoke start").expect("e2e_smoke start");
    sitl.wait_for_log("e2e_smoke task started", Duration::from_secs(2))
        .expect("task started");

    // Give the broker a beat to register the topic. (The first
    // publish triggers the lazy advertise inside Publication.)
    std::thread::sleep(Duration::from_millis(200));

    let status = sitl.shell("uorb status").expect("uorb status");
    let line = status
        .lines()
        .find(|l| l.starts_with("airspeed "))
        .unwrap_or_else(|| panic!("no `airspeed` row in uorb status:\n{status}"));

    // Expected columns:  TOPIC NAME   INST  #SUB  #Q  SIZE  PATH
    // SITL has airspeed_selector + airspeed_validated subscribed by
    // default — at least one of those should be live, so #SUB ≥ 1.
    let cols: Vec<&str> = line.split_whitespace().collect();
    assert!(cols.len() >= 6, "unexpected uorb status row: {line}");
    let n_sub: u32 = cols[2].parse().unwrap_or(0);
    assert!(
        n_sub >= 1,
        "expected at least one subscriber on airspeed (PX4's stock \
         airspeed_selector should be one), got row: {line}"
    );
}

#[test]
fn listener_airspeed_reads_back_rust_publish() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");
    sitl.shell("e2e_smoke start").expect("e2e_smoke start");
    sitl.wait_for_log("e2e_smoke task started", Duration::from_secs(2))
        .expect("task started");
    std::thread::sleep(Duration::from_millis(200));

    let listener = sitl.shell("listener airspeed").expect("listener airspeed");
    assert!(
        !listener.contains("never published"),
        "listener says never published — canonical orb_metadata \
         resolution must have failed:\n{listener}"
    );
    // Field-by-field spot check: confidence is hard-coded to 1.0 in
    // the e2e_smoke task body, so it survives all the way through
    // (Rust struct → uORB broker → listener formatter).
    assert!(
        listener.contains("indicated_airspeed_m_s"),
        "no Airspeed payload formatting in listener output:\n{listener}"
    );
    assert!(
        listener.contains("confidence: 1.00000"),
        "confidence field didn't round-trip with the value the Rust \
         task wrote (expected 1.00000), full output:\n{listener}"
    );
}
