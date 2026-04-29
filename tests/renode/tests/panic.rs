//! Phase-13.6 port of `tests/sitl/tests/panic.rs`, adapted to
//! NuttX semantics.
//!
//! `e2e_panic` spawns a task that panics on first poll;
//! `px4_log::panic_handler!()` logs via `px4_log_modulename(PANIC, …)`
//! and calls `abort()`. On POSIX SITL `abort()` raises SIGABRT and
//! the daemon exits non-zero. On NuttX `abort()` from a worker task
//! kills only that task — the rest of the firmware (nsh, uorb, the
//! WorkQueueManager itself) keeps running. So the SITL-side
//! `wait_for_exit` assertion doesn't apply here.
//!
//! What does survive the platform difference: the panic body landing
//! in the log buffer, formatted by `px4_log_modulename(PANIC, …)`.
//! That's the strongest signal that `panic_handler!()` actually
//! routed through `px4_log` — the rest is platform plumbing.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
fn panic_logs_through_px4_log() {
    ensure_renode!();
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!("[SKIPPED] PX4_RENODE_HAS_PX4 not set");
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    // Best-effort: by the time the worker task panics, this shell()
    // call may have already returned or may time out waiting for the
    // prompt because the worker task was killed mid-stride. Either is
    // fine — we care about what landed in the UART log.
    let _ = sitl.shell("e2e_panic start");

    sitl.wait_for_log("e2e_panic deliberate panic", Duration::from_secs(5))
        .expect(
            "panic body never landed in the firmware log — \
             panic_handler!() didn't route through px4_log",
        );
}
