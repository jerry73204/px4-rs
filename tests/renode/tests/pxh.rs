//! Phase-13 PX4-shell tests. Drive the firmware's `pxh` / `nsh`
//! shell to exercise the runtime crates linked into the firmware.
//!
//! Gated behind a lighter env knob so they only run when the
//! firmware contains the PX4 modules we want to interrogate
//! (`uorb status`, our externals, …). Bare-NuttX builds that just
//! have `nsh` skip these tests; full PX4-on-NuttX builds (phase
//! 13.1) flip the env var on and the tests start running.
//!
//! These also assume the firmware doesn't immediately panic on an
//! unhandled IRQ post-boot. Stock NuttX `nucleo-h743zi:nsh` does
//! exactly that on Renode (MDIOS IRQ from the emulator's
//! reset-default state). A PX4 board config that disables MDIOS
//! and similar unmodelled peripherals at the Kconfig level avoids
//! the issue.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

/// Skip tests in this file unless the firmware advertises the PX4
/// shell suite. Set `PX4_RENODE_HAS_PX4=1` after building a full
/// PX4-on-NuttX firmware (phase 13.1).
fn ensure_pxh() -> bool {
    if std::env::var_os("PX4_RENODE_HAS_PX4").is_none() {
        eprintln!(
            "[SKIPPED] PX4_RENODE_HAS_PX4 not set — skipping pxh-shell tests. \
             See phase-13 docs."
        );
        return false;
    }
    true
}

#[test]
fn shell_uorb_status_returns_topic_table() {
    ensure_renode!();
    if !ensure_pxh() {
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    // Without an rcS startup script, uorb isn't auto-started. Bring
    // it up by hand — `uorb start` is part of the systemcmd we link
    // into the renode-h743 firmware via `default.px4board`.
    sitl.shell("uorb start").expect("uorb start");

    let status = sitl.shell("uorb status").expect("uorb status");
    assert!(
        status.contains("TOPIC NAME") || status.contains("Topics"),
        "uorb status output didn't look like a topic table: {status}"
    );
}

#[test]
fn wait_for_log_picks_up_late_lines() {
    ensure_renode!();
    if !ensure_pxh() {
        return;
    }
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("uorb start").expect("uorb start");
    sitl.shell("uorb status").expect("uorb status");
    sitl.wait_for_log("TOPIC NAME", Duration::from_secs(2))
        .expect("uorb status should have produced a header line");
}
