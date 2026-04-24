//! PX4 logging for Rust modules.
//!
//! `px4-log` expands `info!`/`warn!`/`err!`/`debug!` to PX4's
//! `px4_log_modulename` with a stack-rendered message — no heap, no
//! `alloc`, `no_std`. An optional `log` crate backend routes records
//! from third-party crates through the same path, and an optional
//! `#[panic_handler]` lets pure-Rust PX4 modules handle their own
//! panics.
//!
//! # Declaring a module name
//!
//! PX4's logger prefixes each line with a module name. Rust modules
//! declare theirs once at the crate root:
//!
//! ```ignore
//! use px4_log::module;
//! module!("rate_ctrl");
//! ```
//!
//! The macros above look up `MODULE_NAME` at the call site, so every
//! crate using `px4-log` needs exactly one `module!()` invocation.
//!
//! # Example
//!
//! ```ignore
//! use px4_log::{module, info, warn, err};
//!
//! module!("my_module");
//!
//! fn run() {
//!     info!("sensor ready, rate = {} Hz", 200);
//!     warn!("drift exceeded threshold");
//!     err!("calibration failed: {}", "timeout");
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use core::ffi::c_int;
use core::ffi::{CStr, c_char};
use core::fmt::{self, Write};

// Panic handler lives in the user's module crate via the
// `px4_log::panic_handler!()` macro. See the macro docs below.

#[cfg(feature = "log")]
mod log_backend;
#[cfg(feature = "log")]
pub use log_backend::init;

/// PX4 log levels. Values match `_PX4_LOG_LEVEL_*` in `px4_platform_common/log.h`.
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Level {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
    Panic = 4,
}

/// Stack buffer size for a single formatted message. Matches PX4's
/// typical MAVLink-statustext line length plus headroom.
const BUF_LEN: usize = 256;

struct StackBuf {
    buf: [u8; BUF_LEN],
    pos: usize,
}

impl StackBuf {
    const fn new() -> Self {
        Self {
            buf: [0; BUF_LEN],
            pos: 0,
        }
    }

    /// Null-terminate in place and return a `*const c_char`. Always
    /// leaves at least one byte for the terminator; the message is
    /// truncated if it exceeds `BUF_LEN - 1`.
    #[cfg_attr(feature = "std", allow(dead_code))]
    fn as_c_str(&mut self) -> *const c_char {
        let nul_pos = self.pos.min(BUF_LEN - 1);
        self.buf[nul_pos] = 0;
        self.buf.as_ptr().cast()
    }
}

impl Write for StackBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let room = (BUF_LEN - 1).saturating_sub(self.pos);
        let n = bytes.len().min(room);
        self.buf[self.pos..self.pos + n].copy_from_slice(&bytes[..n]);
        self.pos += n;
        // Silently truncate rather than fail — a log line is best-effort.
        Ok(())
    }
}

/// Internal entry point used by the `info!`/`warn!`/`err!`/`debug!`
/// macros. Not part of the stable API; wrap it if you need direct
/// access.
#[doc(hidden)]
pub fn __log_impl(level: Level, module: &CStr, args: fmt::Arguments<'_>) {
    #[cfg(feature = "std")]
    {
        let tag = match level {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Panic => "PANIC",
        };
        let name = module.to_str().unwrap_or("?");
        let mut buf = StackBuf::new();
        let _ = buf.write_fmt(args);
        let msg = core::str::from_utf8(&buf.buf[..buf.pos]).unwrap_or("<non-utf8>");
        eprintln!("[{tag}] {name}: {msg}");
    }
    #[cfg(not(feature = "std"))]
    {
        let mut buf = StackBuf::new();
        let _ = buf.write_fmt(args);
        let ptr = buf.as_c_str();
        // SAFETY: `px4_log_modulename` is a C variadic accepting
        // (level, const char*, const char*, ...). We pass one trailing
        // argument of type `const char*`, matching the `%s` specifier.
        // `module` and `buf` live until this call returns.
        unsafe {
            px4_sys::px4_log_modulename(level as c_int, module.as_ptr(), c"%s".as_ptr(), ptr);
        }
    }
}

/// Install a `#[panic_handler]` in the invoking crate that logs via
/// `px4_log_modulename(PANIC, …)` and calls libc `abort()`.
///
/// Call exactly once at a PX4 Rust module's crate root:
///
/// ```ignore
/// px4_log::panic_handler!();
/// ```
///
/// Why a macro instead of a feature flag on `px4-log`: the
/// `#[panic_handler]` attribute is global per final binary, and Rust
/// activates it based on the *containing* crate's cfg. If `px4-log`
/// itself defined the handler conditionally, any build that pulls
/// `px4-log` in with the feature on (e.g. `cargo clippy --all-targets`
/// against a workspace example) would get the handler in contexts
/// where `std` already provides one, leading to duplicate-lang-item
/// errors. Scoping the handler to the user's module crate sidesteps
/// that entirely and matches the real-world contract: exactly one
/// Rust PX4 module per firmware may install a panic handler.
#[macro_export]
macro_rules! panic_handler {
    () => {
        // `cargo test` / `cargo clippy --all-targets` compiles a test
        // harness that pulls in `std`, whose own `#[panic_handler]`
        // would collide with ours. `cfg_attr(not(test), …)` keeps
        // the function defined in both cases but only marks it as
        // THE panic handler when we're not in a test build.
        #[cfg_attr(not(test), panic_handler)]
        #[allow(dead_code)]
        fn __px4_rs_on_panic(info: &::core::panic::PanicInfo<'_>) -> ! {
            unsafe extern "C" {
                fn abort() -> !;
            }
            let m: &::core::ffi::CStr = c"panic";
            $crate::__log_impl($crate::Level::Panic, m, ::core::format_args!("{info}"));
            // SAFETY: libc abort diverges. PX4 links newlib on
            // NuttX and glibc on POSIX-SITL — both provide it.
            unsafe { abort() }
        }
    };
}

/// Declare this crate's PX4 module name. Call once at the crate root.
/// Expands to a `const MODULE_NAME: &CStr` that the log macros find by
/// name resolution at the call site.
#[macro_export]
macro_rules! module {
    ($name:literal) => {
        const MODULE_NAME: &::core::ffi::CStr = {
            // SAFETY: the concatenated byte string ends in `\0` and
            // contains no interior NULs because `$name` is a string
            // literal written by the user (no `\0` in ordinary source).
            unsafe {
                ::core::ffi::CStr::from_bytes_with_nul_unchecked(
                    ::core::concat!($name, "\0").as_bytes(),
                )
            }
        };
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::__log_impl($crate::Level::Info, MODULE_NAME, ::core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::__log_impl($crate::Level::Warn, MODULE_NAME, ::core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::__log_impl($crate::Level::Error, MODULE_NAME, ::core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::__log_impl($crate::Level::Debug, MODULE_NAME, ::core::format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    module!("px4_log_test");

    #[test]
    fn stack_buf_truncates_rather_than_fails() {
        let mut b = StackBuf::new();
        let long = "x".repeat(BUF_LEN * 2);
        b.write_str(&long).unwrap();
        assert_eq!(b.pos, BUF_LEN - 1);
        // as_c_str nul-terminates within the buffer.
        let _ = b.as_c_str();
        assert_eq!(b.buf[BUF_LEN - 1], 0);
    }

    #[test]
    fn macros_compile_and_format() {
        // std feature routes to eprintln — safe to run in tests.
        info!("n = {}", 42);
        warn!("drift {:.2}", 1.5);
        err!("failed: {}", "timeout");
        debug!("trace");
    }

    #[test]
    fn module_macro_produces_c_str() {
        assert_eq!(MODULE_NAME.to_bytes(), b"px4_log_test");
    }
}
