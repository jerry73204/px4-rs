//! `ModuleResult` — Termination-style trait for PX4 module entry
//! points.
//!
//! PX4's POSIX dispatcher
//! (`platforms/posix/src/px4/common/px4_daemon/pxh.cpp:113-119`) treats
//! the `<name>_main` return value as plain Unix-y `0 = success /
//! non-zero = failure`. There's no convention for specific error codes
//! (e.g. 2 = usage); the dispatcher prints `Command 'X' failed,
//! returned N.` for any non-zero. The real human-readable error is the
//! module's responsibility, by convention through `px4_log_modulename`
//! — i.e. our `err!` macro.
//!
//! That fixes the Termination shape: `Err(e: Display)` →
//! `__log_impl(Level::Error, module, "{e}")` + return 1.
//!
//! ```ignore
//! #[px4::main]
//! fn main(args: px4::Args) -> Result<(), &'static str> {
//!     match args.subcommand() {
//!         Some(b"start")  => Ok(()),
//!         Some(b"stop")   => Ok(()),
//!         _               => Err("usage: hello_module {start|stop}"),
//!     }
//! }
//! ```

use core::ffi::{CStr, c_int};

use crate::{__log_impl, Level};

/// Convert a PX4 module entry point's return value into the C `int`
/// PX4 expects.
///
/// The trait is open: implement it for your own return type if `()`,
/// `c_int`, or `Result<T, E: Display>` doesn't fit. The macro
/// `#[px4::main]` emits `ModuleResult::into_c_int(result, MODULE_NAME)`
/// — the `module` argument lets the `Result` impl log the error
/// through `px4_log_modulename` with the calling module's name.
pub trait ModuleResult {
    /// `module` is the calling module's name (`MODULE_NAME` in the
    /// macro-emitted entry). Only the `Result` impl uses it; trivial
    /// impls ignore the argument.
    fn into_c_int(self, module: &CStr) -> c_int;
}

impl ModuleResult for () {
    fn into_c_int(self, _module: &CStr) -> c_int {
        0
    }
}

impl ModuleResult for c_int {
    fn into_c_int(self, _module: &CStr) -> c_int {
        self
    }
}

impl<T: ModuleResult, E: core::fmt::Display> ModuleResult for Result<T, E> {
    fn into_c_int(self, module: &CStr) -> c_int {
        match self {
            Ok(t) => t.into_c_int(module),
            Err(e) => {
                __log_impl(Level::Error, module, format_args!("{e}"));
                1
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    fn cstr() -> &'static CStr {
        c"px4_log_test"
    }

    #[test]
    fn unit_returns_zero() {
        assert_eq!(().into_c_int(cstr()), 0);
    }

    #[test]
    fn c_int_passes_through() {
        assert_eq!((42 as c_int).into_c_int(cstr()), 42);
        assert_eq!((-7 as c_int).into_c_int(cstr()), -7);
    }

    #[test]
    fn ok_unit_is_zero() {
        let r: Result<(), &'static str> = Ok(());
        assert_eq!(r.into_c_int(cstr()), 0);
    }

    #[test]
    fn err_logs_and_returns_one() {
        // The std impl of __log_impl writes to stderr; we can't easily
        // capture that in a unit test, but the return value is what
        // the macro emits.
        let r: Result<(), &'static str> = Err("nope");
        assert_eq!(r.into_c_int(cstr()), 1);
    }

    #[test]
    fn nested_result_recurses() {
        // Result<c_int, _> on Ok passes the int through; on Err logs
        // and returns 1. Used by callers who want explicit non-zero
        // success codes.
        let r: Result<c_int, &'static str> = Ok(7);
        assert_eq!(r.into_c_int(cstr()), 7);
        let r: Result<c_int, &'static str> = Err("nope");
        assert_eq!(r.into_c_int(cstr()), 1);
    }
}
