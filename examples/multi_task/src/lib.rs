//! `multi_task` — two `#[task]`s on different WorkQueues talking via `Notify`.
//!
//! Idiomatic PX4 split: one task does the time-driven nudging and
//! another does the heavier work, each on its own WQ thread so they
//! can preempt independently. The producer runs on `hp_default` and
//! pings a `Notify` once a second; the consumer runs on `lp_default`,
//! awaits the notification, and bumps a counter.
//!
//! Run with:
//!
//! ```text
//! pxh> multi_task start
//! INFO  [multi_task] producer started
//! INFO  [multi_task] consumer started
//! INFO  [multi_task] consumer woke, count=1
//! INFO  [multi_task] consumer woke, count=2
//! …
//! ```

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};
use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

use px4_log::{info, module, panic_handler};
use px4_workqueue::{Notify, sleep, task};

module!("multi_task");
panic_handler!();

static SIGNAL: Notify = Notify::new();
static WAKES: AtomicU32 = AtomicU32::new(0);

#[task(wq = "hp_default")]
async fn producer() {
    info!("producer started");
    loop {
        sleep(Duration::from_secs(1)).await;
        SIGNAL.notify();
    }
}

#[task(wq = "lp_default")]
async fn consumer() {
    info!("consumer started");
    loop {
        SIGNAL.notified().await;
        let n = WAKES.fetch_add(1, Ordering::AcqRel) + 1;
        info!("consumer woke, count={n}");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn multi_task_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => {
            // Spawn the consumer first so a producer notify that
            // races with the consumer's first poll always lands as a
            // stored permit, never on a not-yet-registered waiter.
            let cons = match consumer::try_spawn() {
                Ok(t) => t,
                Err(_) => {
                    px4_log::err!("consumer already running");
                    return 1;
                }
            };
            let prod = match producer::try_spawn() {
                Ok(t) => t,
                Err(_) => {
                    px4_log::err!("producer already running");
                    return 1;
                }
            };
            cons.forget();
            prod.forget();
            info!("started");
            0
        }
        Some(b"status") => {
            let n = WAKES.load(Ordering::Acquire);
            info!("running, wake count={n}");
            0
        }
        Some(b"stop") => {
            info!("stop is a no-op in this example");
            0
        }
        _ => {
            px4_log::err!("usage: multi_task {{start|stop|status}}");
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
