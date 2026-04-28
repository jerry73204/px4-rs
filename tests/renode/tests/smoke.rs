//! Phase-13 smoke test — boots PX4 + NuttX on Renode and confirms
//! the pxh shell comes up. Mirrors the shape of
//! `tests/sitl/tests/boot.rs` so the two e2e tracks have parallel
//! coverage at the smoke level.
//!
//! This test reports `[SKIPPED]` until the phase-13 prerequisites
//! are in place: the `RENODE` env var must point at a working
//! `renode` binary, and `PX4_RENODE_FIRMWARE` must point at a
//! built `px4_renode_h743.elf`. See
//! `docs/roadmap/phase-13-renode-nuttx-e2e.md` for the build steps.

use std::time::Duration;

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
fn fixture_boots_and_reaches_pxh_prompt() {
    ensure_renode!();
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    // Once `Startup script returned successfully` lands (the boot
    // gate inside `Px4RenodeSitl::boot`), the shell is up. As an
    // extra sanity check, run a no-op command and look for the
    // prompt round-tripping.
    let out = sitl.shell("ver").expect("ver shell command");
    assert!(
        !out.is_empty(),
        "ver returned empty output, expected version banner: snapshot:\n{}",
        sitl.log_snapshot()
    );
}

#[test]
fn shell_uorb_status_returns_topic_table() {
    ensure_renode!();
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    let status = sitl.shell("uorb status").expect("uorb status");
    assert!(
        status.contains("TOPIC NAME") || status.contains("Topics"),
        "uorb status output didn't look like a topic table: {status}"
    );
}

#[test]
fn wait_for_log_picks_up_late_lines() {
    ensure_renode!();
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    sitl.shell("uorb status").expect("uorb status");

    // Line emitted by uorb status output. wait_for_log should
    // resolve immediately because the line already landed.
    sitl.wait_for_log("TOPIC NAME", Duration::from_secs(2))
        .expect("uorb status should have produced a header line");
}
