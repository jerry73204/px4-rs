//! Phase-13.6 port of `tests/sitl/tests/smoke.rs::e2e_smoke_*`.
//!
//! Boots renode-h743 firmware, starts the `e2e_smoke` Rust module,
//! confirms the publication makes it into uORB, and reads it back
//! through the canonical `listener` path. Exercises the full
//! `cargo + cc + PX4 link + uORB broker` chain on real ARM Cortex-M7
//! codegen + NuttX scheduling.
//!
//! `e2e_smoke`'s task body is a tight `loop { publish; yield_now().await; }`.
//! On NuttX `yield_now()` calls `usleep(1)` after waking, which forces
//! the kernel scheduler to run; without that the WQ thread monopolises
//! the CPU and nsh's prompt never lands.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

fn ensure_pxh() -> bool {
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!(
            "[SKIPPED] PX4_RENODE_HAS_PX4 not set — needs PX4-on-NuttX firmware."
        );
        return false;
    }
    true
}

#[test]
fn e2e_smoke_starts_and_logs() {
    ensure_renode!();
    if !ensure_pxh() {
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    let out = sitl.shell("e2e_smoke start").expect("e2e_smoke start");
    assert!(
        out.contains("started"),
        "expected `e2e_smoke start` to log 'started', got:\n{out}"
    );

    let task_banner = sitl
        .wait_for_log("e2e_smoke task started", Duration::from_secs(3))
        .expect("task body never reached its banner — #[task] didn't run");
    assert!(task_banner.contains("e2e_smoke task started"));
}

#[test]
fn airspeed_topic_appears_in_uorb_status() {
    ensure_renode!();
    if !ensure_pxh() {
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");
    sitl.shell("e2e_smoke start").expect("e2e_smoke start");
    sitl.wait_for_log("e2e_smoke task started", Duration::from_secs(3))
        .expect("task started");

    std::thread::sleep(Duration::from_millis(300));

    let status = sitl.shell("uorb status").expect("uorb status");
    let line = status
        .lines()
        .find(|l| l.starts_with("airspeed "))
        .unwrap_or_else(|| panic!("no `airspeed` row in uorb status:\n{status}"));
    let cols: Vec<&str> = line.split_whitespace().collect();
    assert!(cols.len() >= 6, "unexpected uorb status row: {line}");
}

#[test]
fn listener_airspeed_reads_back_rust_publish() {
    ensure_renode!();
    if !ensure_pxh() {
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");
    sitl.shell("e2e_smoke start").expect("e2e_smoke start");
    sitl.wait_for_log("e2e_smoke task started", Duration::from_secs(3))
        .expect("task started");
    std::thread::sleep(Duration::from_millis(300));

    let listener = sitl.shell("listener airspeed").expect("listener airspeed");
    assert!(
        !listener.contains("never published"),
        "listener says never published — canonical orb_metadata \
         resolution must have failed:\n{listener}"
    );
    assert!(listener.contains("indicated_airspeed_m_s"));
    assert!(listener.contains("confidence: 1.00000"));
}
