//! `e2e_multi_wq` — phase-11 multi-WorkQueue smoke test.
//!
//! Two `#[task]` entries pinned to different WorkQueues each tick a
//! counter and log a banner once per second. The test in
//! `tests/multi_wq.rs` asserts that both banners appear, proving that
//! the runtime drives more than one WQ thread independently.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};

use px4_log::{info, module, panic_handler};
use px4_workqueue::{task, yield_now};

module!("e2e_multi_wq");
panic_handler!();

#[task(wq = "lp_default")]
async fn lp_tick() {
    info!("lp_default tick alive");
    let mut n: u32 = 0;
    loop {
        n = n.wrapping_add(1);
        if n.is_multiple_of(50_000) {
            info!("lp_default tick n={n}");
        }
        yield_now().await;
    }
}

#[task(wq = "hp_default")]
async fn hp_tick() {
    info!("hp_default tick alive");
    let mut n: u32 = 0;
    loop {
        n = n.wrapping_add(1);
        if n.is_multiple_of(50_000) {
            info!("hp_default tick n={n}");
        }
        yield_now().await;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn e2e_multi_wq_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => {
            let lp = match lp_tick::try_spawn() {
                Ok(t) => t,
                Err(_) => {
                    px4_log::err!("lp_tick already running");
                    return 1;
                }
            };
            let hp = match hp_tick::try_spawn() {
                Ok(t) => t,
                Err(_) => {
                    px4_log::err!("hp_tick already running");
                    return 1;
                }
            };
            lp.forget();
            hp.forget();
            info!("started");
            0
        }
        Some(b"status") => {
            info!("running");
            0
        }
        Some(b"stop") => {
            info!("stop is a no-op in this test module");
            0
        }
        _ => {
            px4_log::err!("usage: e2e_multi_wq {{start|stop|status}}");
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

