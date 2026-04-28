//! Phase-13 boot smoke. Confirms the firmware referenced by
//! `PX4_RENODE_FIRMWARE` boots far enough to print NuttX's
//! `NuttShell` banner over UART3, with Renode wired up by our
//! `.repl`/`.resc` scripts.
//!
//! When `PX4_RENODE_FIRMWARE` points at a stock NuttX `nsh` build,
//! that's all this test exercises — the firmware is known to hit
//! `irq_unexpected_isr → PANIC()` shortly after the prompt prints
//! (NuttX-on-Renode-H743 has an unhandled MDIOS IRQ from the
//! emulator's reset-default state). Boot-banner detection happens
//! before the panic, so this test is reliable.
//!
//! When `PX4_RENODE_FIRMWARE` points at a full PX4-on-NuttX build
//! (phase-13.1 work item), the same boot-banner check still holds
//! and the richer shell tests in `pxh.rs` start exercising the
//! runtime crates.

use px4_renode_tests::{Px4RenodeSitl, ensure_renode};

#[test]
fn fixture_boots_to_nuttx_banner() {
    ensure_renode!();
    // Boot itself blocks until the NuttShell banner lands; reaching
    // here means Renode came up, the .repl loaded, the .resc wired
    // UART3 to the host pty, the firmware loaded, NuttX initialised
    // through the H7 PWR/RCC sequence (via our PWR mock), and
    // userspace started. Anything earlier failing would have
    // surfaced as TestError::BootTimeout.
    let sitl = Px4RenodeSitl::boot().expect("boot Renode");

    let snapshot = sitl.log_snapshot();
    assert!(
        snapshot.contains("NuttShell"),
        "boot banner missing from log; snapshot:\n{snapshot}"
    );
}
