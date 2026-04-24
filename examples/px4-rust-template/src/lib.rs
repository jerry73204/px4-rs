//! Skeleton PX4 module — copy this directory, rename `px4_rust_template`
//! everywhere, and start adding tasks.
//!
//! The module exposes one C entry point (`px4_rust_template_main`)
//! which the generated CMake shim forwards to from PX4's shell. On
//! `start`, it spawns a single `#[task]` that prints a hello message
//! and exits.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};

use px4_log::{info, module, panic_handler};
use px4_workqueue::task;

module!("px4_rust_template");
panic_handler!();

#[task(wq = "lp_default")]
async fn hello() {
    info!("hello from a Rust PX4 module");
}

/// PX4 shell entry point. Argument convention follows PX4's typical
/// modules: `start` / `stop` / `status`.
///
/// Returns 0 on success, non-zero on usage error.
#[unsafe(no_mangle)]
pub extern "C" fn px4_rust_template_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let cmd = parse_first_arg(argc, argv);
    match cmd {
        Some(b"start") => match hello::try_spawn() {
            Ok(token) => {
                token.forget();
                0
            }
            Err(_) => 1,
        },
        Some(b"stop") | Some(b"status") => 0, // not implemented in the skeleton
        _ => {
            // PX4 prints whatever PX4_ERR! emits.
            px4_log::err!("usage: px4_rust_template {{start|stop|status}}");
            1
        }
    }
}

fn parse_first_arg<'a>(argc: c_int, argv: *mut *mut c_char) -> Option<&'a [u8]> {
    if argc < 2 || argv.is_null() {
        return None;
    }
    // SAFETY: argv[1] is a NUL-terminated C string supplied by PX4's shell.
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
