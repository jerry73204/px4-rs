//! End-to-end test infrastructure for px4-rs against Renode-emulated
//! PX4 + NuttX on STM32H7.
//!
//! The crate is **not** a member of the main `px4-rs` workspace. It
//! has its own `Cargo.toml`, `rust-toolchain.toml`, and nextest
//! config — same pattern as `tests/sitl/`. This crate uses `std`
//! and `nix`; the runtime crates are `no_std`.
//!
//! Run the suite with:
//!
//! ```sh
//! cd tests/renode
//! PX4_RENODE_FIRMWARE=$HOME/.../px4_renode_h743.elf \
//!     RENODE=/usr/bin/renode \
//!     cargo nextest run
//! ```
//!
//! Without `RENODE` and `PX4_RENODE_FIRMWARE` set, every test
//! reports `[SKIPPED]` rather than failing — same shape as
//! `tests/sitl/`'s `ensure_px4!()`.

pub mod fixtures;
pub mod process;

pub use fixtures::Px4RenodeSitl;

/// Soft-skip the current test with a reason. Prints `[SKIPPED]
/// <reason>` to stderr and returns from the containing function.
#[macro_export]
macro_rules! skip {
    ($($arg:tt)*) => {{
        ::std::eprintln!("[SKIPPED] {}", ::std::format_args!($($arg)*));
        return;
    }};
}

/// Shorthand: skip the test if `RENODE` (path to the renode binary)
/// or `PX4_RENODE_FIRMWARE` (path to the px4 .elf to boot) is not
/// configured.
///
/// Use at the top of every full-boot Renode test:
///
/// ```ignore
/// #[test]
/// fn my_renode_test() {
///     ensure_renode!();
///     let sitl = Px4RenodeSitl::boot()?;
///     // …
/// }
/// ```
#[macro_export]
macro_rules! ensure_renode {
    () => {
        if !$crate::fixtures::renode_available() {
            $crate::skip!(
                "RENODE / PX4_RENODE_FIRMWARE not set or missing — \
                 see docs/roadmap/phase-13-renode-nuttx-e2e.md for setup"
            );
        }
    };
}

/// Lighter sibling of [`ensure_renode!`]: skip only if `RENODE`
/// (the binary path) is missing. Tests that exercise the
/// `.repl` / `.resc` plumbing without booting firmware — see
/// [`fixtures::probe_platform`] — should gate on this. Lets
/// platform-load smoke tests run live as soon as Renode itself is
/// installed, without waiting on phase-13 work item 13.1.
#[macro_export]
macro_rules! ensure_renode_binary {
    () => {
        if !$crate::fixtures::renode_binary_available() {
            $crate::skip!("RENODE not set or missing — `just setup-renode` to install");
        }
    };
}

/// Standard error type for fixture failures.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("nix: {0}")]
    Nix(#[from] nix::Error),

    #[error("RENODE binary not set or missing")]
    NoRenode,

    #[error("PX4_RENODE_FIRMWARE not set or missing")]
    NoFirmware,

    #[error("Renode did not boot to `pxh>` within {timeout_secs}s")]
    BootTimeout { timeout_secs: u64 },

    #[error("expected log pattern `{pattern}` not seen within {timeout_secs}s")]
    LogTimeout { pattern: String, timeout_secs: u64 },

    #[error("subprocess `{cmd}` exited non-zero ({status})")]
    SubprocessFailed { cmd: String, status: i32 },
}

pub type Result<T> = std::result::Result<T, TestError>;
