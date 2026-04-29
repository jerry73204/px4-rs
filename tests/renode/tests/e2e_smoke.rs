//! Phase-13.6 port of `tests/sitl/tests/smoke.rs::e2e_smoke_*`.
//!
//! ## Currently `#[ignore]`d
//!
//! The `e2e_smoke` module body is a tight `loop { publish; yield_now().await; }`.
//! On POSIX SITL the pthread that runs `lp_default` shares CPU
//! cooperatively with the rest of the daemon (lockstep_scheduler),
//! and `yield_now()` plus pthread preemption gives nsh enough time
//! to drain its TX buffer and print `nsh>`. On NuttX,
//! `yield_now()`'s `ScheduleNow → re-poll` round-trip happens
//! entirely inside the lp_default WorkQueue thread without ever
//! relinquishing to the kernel scheduler, so the WQ thread monopolises
//! the CPU and nsh's prompt never lands. Effects:
//!
//!  - `e2e_smoke start`'s `info!("started")` from main never makes
//!    it to the UART (stdio mutex is held by the WQ thread that's
//!    busy printing the task body's banner first).
//!  - The shell's wait-for-prompt times out.
//!  - Any subsequent shell command on the same fixture also times out.
//!
//! The fix lives in `px4-workqueue` — `yield_now()` needs to ride a
//! 0-µs HRT timer or call `sched_yield` on NuttX so the OS scheduler
//! gets a chance. Tracked separately from phase-13.6 so the rest of
//! the SITL test surface can land on Renode without waiting on it.
//!
//! The first test (`e2e_smoke_starts_and_logs`) is the canary: once
//! `yield_now()` becomes scheduler-fair on NuttX, drop the `#[ignore]`
//! and the others (`airspeed_topic_appears_in_uorb_status`,
//! `listener_airspeed_reads_back_rust_publish`) should pass at the
//! same time without other changes.

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
#[ignore = "yield_now starves nsh on NuttX — see file-level docs"]
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
#[ignore = "depends on e2e_smoke — see file-level docs"]
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
#[ignore = "depends on e2e_smoke — see file-level docs"]
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
