//! `e2e_panic` — phase-11 panic-handler smoke test.
//!
//! On `start`, spawns one task on `lp_default` that panics on its
//! first poll. The `panic_handler!()` macro routes the panic message
//! through `px4_log` and then calls libc `abort()`, which terminates
//! the SITL daemon non-zero. Used by `tests/panic.rs` to verify both
//! halves of that path (the log line, and the daemon exit).

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};

use px4_log::{info, module, panic_handler};
use px4_workqueue::task;

module!("e2e_panic");
panic_handler!();

#[task(wq = "lp_default")]
async fn boom() {
    info!("e2e_panic task about to panic");
    panic!("e2e_panic deliberate panic");
}

#[unsafe(no_mangle)]
pub extern "C" fn e2e_panic_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => match boom::try_spawn() {
            Ok(token) => {
                token.forget();
                info!("started");
                0
            }
            Err(_) => {
                px4_log::err!("already running");
                1
            }
        },
        Some(b"status") => {
            info!("running");
            0
        }
        Some(b"stop") => {
            info!("stop is a no-op in this test module");
            0
        }
        _ => {
            px4_log::err!("usage: e2e_panic {{start|stop|status}}");
            1
        }
    }
}

fn parse_first_arg<'a>(argc: c_int, argv: *mut *mut c_char) -> Option<&'a [u8]> {
    if argc < 2 || argv.is_null() {
        return None;
    }
    unsafe {
        let s = *argv.add(1);
        if s.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *s.add(len) != 0 {
            len += 1;
            if len > 64 {
                return None;
            }
        }
        Some(core::slice::from_raw_parts(s as *const u8, len))
    }
}
