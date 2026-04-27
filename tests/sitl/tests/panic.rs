//! Phase-11 work item 11.8 — `panic_handler!()` round-trip in SITL.
//!
//! The `e2e_panic` module spawns a task that panics on first poll.
//! `px4_log::panic_handler!()` should log the panic message via
//! `px4_log_modulename(PANIC, …)` and then call libc `abort()`,
//! which terminates the SITL daemon non-zero. We assert both halves
//! of that path: the log line and the daemon exit.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn panic_logs_and_aborts_daemon() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");

    // The shell command itself may or may not return zero — by the
    // time the daemon's WQ polls the panicking task, the daemon is
    // gone and the client may see EOF. We don't care; we care about
    // what landed in the log buffer and the daemon's exit status.
    let _ = sitl.shell("e2e_panic start");

    // `panic_handler!()` formats its message with `c"panic"` as the
    // module name. The exact preamble is PX4's stock log formatter,
    // which prefixes the level + module — so the panic body appears
    // as a substring on its own line.
    sitl.wait_for_log("e2e_panic deliberate panic", Duration::from_secs(3))
        .expect(
            "panic body never landed in the daemon log — \
             panic_handler!() didn't route through px4_log",
        );

    // libc::abort() raises SIGABRT, which exits the process with a
    // signal-style status. Plain `success()` would be `false`; we
    // assert on the more specific "did the process leave on its own
    // within the timeout" question.
    let status = sitl
        .wait_for_exit(Duration::from_secs(5))
        .expect("daemon stayed alive after panic — abort() didn't fire");
    assert!(
        !status.success(),
        "daemon exited successfully after panic, expected non-zero / signal-killed: {status:?}"
    );
}
