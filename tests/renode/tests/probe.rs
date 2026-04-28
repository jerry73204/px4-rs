//! Live Renode smoke that doesn't need firmware.
//!
//! Boots Renode, loads the `px4_renode_h743.repl` platform
//! description, quits. Validates that:
//!
//! 1. The Renode subprocess actually starts and produces output.
//! 2. The `.repl` parses without errors (any ambiguous IRQ,
//!    duplicate peripheral, missing CPU type fails fast here).
//! 3. The fixture's spawn / drain / kill plumbing works end-to-end
//!    against a real Renode binary.
//!
//! This runs live as soon as `RENODE` is set — it doesn't wait on
//! work item 13.1 (the firmware build). On a runner without Renode
//! installed, it skip-passes via `ensure_renode_binary!()`.

use std::time::Duration;

use px4_renode_tests::{ensure_renode_binary, fixtures::probe_platform};

#[test]
fn renode_loads_platform_description_cleanly() {
    ensure_renode_binary!();

    let outcome = probe_platform(Duration::from_secs(15)).expect("spawn renode");

    let status = outcome.status.unwrap_or_else(|| {
        panic!(
            "renode didn't quit within 15s; log:\n{}",
            outcome.renode_log
        )
    });

    assert!(
        status.success(),
        "renode exited non-zero ({status:?}) loading the platform; log:\n{}",
        outcome.renode_log
    );

    // Loose sanity checks on the captured log: Renode prints its
    // version banner, then "System bus created.", then the quit
    // notice. Any of those missing means our spawn path or .repl
    // is broken in a way exit-code alone wouldn't catch.
    assert!(
        outcome.renode_log.contains("Renode"),
        "no Renode banner in log: {}",
        outcome.renode_log
    );
    assert!(
        outcome.renode_log.contains("System bus created")
            || outcome.renode_log.contains("Renode is quitting"),
        "no platform-load / quit confirmation in log: {}",
        outcome.renode_log
    );

    // Anti-regression: the platform file used to wire usart2's IRQ
    // ambiguously (Error E15: Ambiguous choice of default
    // interrupt). Catch a re-introduction immediately.
    assert!(
        !outcome.renode_log.contains("Ambiguous choice")
            && !outcome.renode_log.contains("Error E15"),
        ".repl produced an ambiguous-binding error: {}",
        outcome.renode_log
    );
}
