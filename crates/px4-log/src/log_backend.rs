//! `log` crate integration. Routes third-party `log::info!`/etc records
//! through `px4_log_modulename` so dependencies that use the standard
//! logging facade show up in PX4's console alongside our own macros.

use core::ffi::CStr;

use crate::{__log_impl, Level};

struct PX4Logger;

static LOGGER: PX4Logger = PX4Logger;

impl log::Log for PX4Logger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let level = match record.level() {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warn,
            log::Level::Info => Level::Info,
            log::Level::Debug | log::Level::Trace => Level::Debug,
        };

        // `log::Record::target()` is the caller's module path; render
        // the first ≤31 bytes into a stack C-string so PX4's logger
        // gets per-crate attribution for free.
        let mut name_buf = [0u8; 32];
        let src = record.target().as_bytes();
        let n = src.len().min(name_buf.len() - 1);
        name_buf[..n].copy_from_slice(&src[..n]);
        // SAFETY: `name_buf` ends with `\0` (zero-initialised), bytes
        // above `n` are untouched, and byte `n` is therefore 0.
        let module = unsafe { CStr::from_ptr(name_buf.as_ptr().cast()) };

        __log_impl(level, module, *record.args());
    }

    fn flush(&self) {}
}

/// Install the PX4 log backend. Call once at module start. Idempotent
/// within a process — subsequent calls are no-ops.
pub fn init() {
    // `set_logger_racy` is available on `no_std`. Called from a PX4
    // module's single-threaded init path, so "racy" is fine.
    //
    // Ignore the result: if something else already installed a logger
    // (e.g. a parallel test harness) we simply coexist.
    let _ = unsafe { log::set_logger_racy(&LOGGER) };
    log::set_max_level(log::LevelFilter::Info);
}
