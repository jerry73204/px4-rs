//! `heartbeat` — minimal end-to-end PX4 Rust module.
//!
//! Publishes a synthetic `Airspeed` message in a tight loop with
//! `yield_now` between iterations so the WorkQueue can interleave.
//! Demonstrates `#[task]`, `#[px4_message]`, `Publication`, and the
//! CMake helper all at once.
//!
//! The publish rate is uncapped — a 1 Hz rate would need a `Timer`
//! primitive (deferred from phase 04). For the moment, this module
//! exists primarily to exercise the build pipeline.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use px4_log::{info, module};
use px4_msg_macros::px4_message;
use px4_uorb::Publication;
use px4_workqueue::task;

module!("heartbeat");

#[px4_message("Airspeed.msg")]
pub struct Airspeed;

static AIRSPEED_PUB: Publication<airspeed> = Publication::new();

#[task(wq = "lp_default")]
async fn pump() {
    info!("heartbeat task started");
    let mut counter: u64 = 0;
    loop {
        counter = counter.wrapping_add(1);
        let sample = Airspeed {
            timestamp: counter,
            timestamp_sample: counter,
            indicated_airspeed_m_s: 0.0,
            true_airspeed_m_s: 0.0,
            confidence: 1.0,
            _padding0: [0; 4],
        };
        if AIRSPEED_PUB.publish(&sample).is_err() {
            px4_log::err!("publish failed at counter {counter}");
        }
        // Yield so the WorkQueue can run other items between publishes.
        // A real heartbeat would await a Timer.
        YieldOnce::new().await;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn heartbeat_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let cmd = parse_first_arg(argc, argv);
    match cmd {
        Some(b"start") => match pump::try_spawn() {
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
            // Phase 07 doesn't implement clean shutdown; document the
            // limitation rather than pretend.
            info!("stop is not implemented in this example");
            0
        }
        _ => {
            px4_log::err!("usage: heartbeat {{start|stop|status}}");
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

/// Identical to the helper in `px4-uorb/tests/round_trip.rs`. Pulled
/// inline here so the example doesn't have a test-only dependency.
struct YieldOnce(bool);
impl YieldOnce {
    fn new() -> Self {
        Self(false)
    }
}
impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
