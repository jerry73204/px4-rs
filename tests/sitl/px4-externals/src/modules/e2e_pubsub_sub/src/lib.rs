//! `e2e_pubsub_sub` — phase-11 pub/sub subscriber half.
//!
//! Subscribes to the `E2ePubsub` topic published by
//! `e2e_pubsub_pub`. Each `recv().await` produces a log line carrying
//! the counter value, which the test asserts on. Exercises the real
//! `Subscription` path through PX4's broker.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};

use px4_log::{info, module, panic_handler};
use px4_msg_macros::px4_message;
use px4_uorb::Subscription;
use px4_workqueue::task;

module!("e2e_pubsub_sub");
panic_handler!();

#[px4_message("E2ePubsub.msg")]
pub struct E2ePubsub;

#[task(wq = "lp_default")]
async fn drain() {
    info!("e2e_pubsub_sub task started");
    let sub = Subscription::<e2e_pubsub>::new();
    loop {
        let msg: E2ePubsub = sub.recv().await;
        info!("got counter={}", msg.counter);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn e2e_pubsub_sub_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => match drain::try_spawn() {
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
            px4_log::err!("usage: e2e_pubsub_sub {{start|stop|status}}");
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
