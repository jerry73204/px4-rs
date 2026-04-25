//! Smoke test for the `Px4Sitl` fixture itself.
//!
//! Boots a fresh SITL daemon, sends `uorb status`, kills it.
//! No Rust modules are exercised — that's 11.6+. This test just
//! proves the fixture's boot/shell/drop cycle works against a real
//! `make px4_sitl` output.

use std::time::Duration;

use px4_sitl_tests::{Px4Sitl, ensure_px4};

#[test]
fn fixture_boots_and_drops() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");
    // Drop fires when sitl goes out of scope; if SIGTERM/SIGKILL ever
    // hangs we'd hit nextest's slow-timeout, not silently zombie.
    drop(sitl);
}

#[test]
fn shell_uorb_status_returns_topic_table() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");
    let out = sitl.shell("uorb status").expect("uorb status");
    // The header is printed by uorb_status() in PX4 — stable across
    // versions for the entire v1.x line.
    assert!(
        out.contains("TOPIC NAME"),
        "expected `TOPIC NAME` header in `uorb status`, got:\n{out}"
    );
}

#[test]
fn wait_for_log_picks_up_late_lines() {
    ensure_px4!();
    let sitl = Px4Sitl::boot().expect("boot SITL");
    let line = sitl
        .wait_for_log("Startup script returned", Duration::from_secs(1))
        .expect("the boot marker is in the buffer");
    assert!(line.contains("Startup script returned"));
}
