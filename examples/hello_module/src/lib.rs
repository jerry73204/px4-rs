//! `hello_module` — simplest possible px4-rs example.
//!
//! Spawns one `#[task]` on the `lp_default` work queue that prints
//! a `hello` line every second via `px4_workqueue::sleep`. No uORB,
//! no multi-task plumbing — just the minimal scaffold of "task +
//! logger + timer".
//!
//! Run with:
//!
//! ```text
//! pxh> hello_module start
//! INFO  [hello_module] hello tick=1
//! INFO  [hello_module] hello tick=2
//! …
//! ```

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};
use core::time::Duration;

use px4_log::{info, module, panic_handler};
use px4_workqueue::{sleep, task};

module!("hello_module");
panic_handler!();

#[task(wq = "lp_default")]
async fn ticker() {
    info!("ticker started");
    let mut tick: u64 = 0;
    loop {
        tick = tick.wrapping_add(1);
        info!("hello tick={tick}");
        sleep(Duration::from_secs(1)).await;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hello_module_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => match ticker::try_spawn() {
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
            info!("stop is a no-op in this example");
            0
        }
        _ => {
            px4_log::err!("usage: hello_module {{start|stop|status}}");
            1
        }
    }
}

fn parse_first_arg<'a>(argc: c_int, argv: *mut *mut c_char) -> Option<&'a [u8]> {
    if argc < 2 || argv.is_null() {
        return None;
    }
    // SAFETY: argv[1] is a NUL-terminated C string from PX4's shell.
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
