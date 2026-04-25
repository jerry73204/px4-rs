//! End-to-end test infrastructure for px4-rs against PX4 SITL.
//!
//! The crate is **not** a member of the main `px4-rs` workspace. It
//! has its own `Cargo.toml`, `rust-toolchain.toml`, and nextest
//! config. Two reasons:
//!
//!   - It uses `std`, `regex`, `rstest`, etc. — none of which the
//!     main `no_std` workspace wants pulled in transitively.
//!   - PX4 modules under `px4-externals/src/modules/` install
//!     `px4_log::panic_handler!()`, which would conflict with `std`'s
//!     panic handler if those modules were workspace members.
//!
//! Run the suite with:
//!
//! ```sh
//! cd tests/sitl
//! PX4_AUTOPILOT_DIR=$HOME/repos/PX4-Autopilot cargo nextest run
//! ```
//!
//! Without `PX4_AUTOPILOT_DIR`, every test reports `[SKIPPED]` rather
//! than failing.

pub mod fixtures;
pub mod process;

pub use fixtures::Px4Sitl;

/// Soft-skip the current test with a reason.
///
/// Prints `[SKIPPED] <reason>` to stderr and returns from the
/// containing function. Use it inside the test body itself — calling
/// it from a helper function would only return from the helper.
///
/// The test reports as PASS so CI runs stay green when prerequisites
/// (e.g. `PX4_AUTOPILOT_DIR`) are missing; the skip line still shows
/// up in the output for humans.
#[macro_export]
macro_rules! skip {
    ($($arg:tt)*) => {{
        ::std::eprintln!("[SKIPPED] {}", ::std::format_args!($($arg)*));
        return;
    }};
}

/// Shorthand for the standard SITL precondition check. Place at the
/// top of every test that needs a real PX4 checkout:
///
/// ```ignore
/// #[test]
/// fn my_sitl_test() {
///     ensure_px4!();
///     let sitl = Px4Sitl::boot()?;
///     // ...
/// }
/// ```
#[macro_export]
macro_rules! ensure_px4 {
    () => {
        if $crate::fixtures::px4_source_dir().is_err() {
            $crate::skip!("PX4_AUTOPILOT_DIR not set or missing");
        }
    };
}

/// Standard error type for fixture failures.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("PX4_AUTOPILOT_DIR is not set or does not exist")]
    NoPx4Tree,

    #[error("PX4 build failed: {0}")]
    BuildFailed(String),

    #[error("daemon did not become ready within {timeout_secs}s")]
    BootTimeout { timeout_secs: u64 },

    #[error("expected log pattern `{pattern}` not seen within {timeout_secs}s")]
    LogTimeout { pattern: String, timeout_secs: u64 },

    #[error("subprocess `{cmd}` exited non-zero ({status})")]
    SubprocessFailed { cmd: String, status: i32 },
}

pub type Result<T> = std::result::Result<T, TestError>;
