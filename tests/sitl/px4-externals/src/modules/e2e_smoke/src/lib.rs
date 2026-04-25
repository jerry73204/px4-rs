//! `e2e_smoke` — minimal phase-11 PX4 module.
//!
//! On `start`, spawns one task on `lp_default` that publishes
//! `Airspeed` in a tight loop with `yield_now` between iterations.
//! Used by `tests/smoke.rs` (work item 11.6) to assert that the full
//! cargo + cc + PX4 link path produces a binary that can register a
//! topic with the uORB broker.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use px4_log::{info, module, panic_handler};
use px4_msg_macros::px4_message;
use px4_uorb::Publication;
use px4_workqueue::task;

module!("e2e_smoke");
panic_handler!();

#[px4_message("Airspeed.msg")]
pub struct Airspeed;

static AIRSPEED_PUB: Publication<airspeed> = Publication::new();

#[task(wq = "lp_default")]
async fn pump() {
    info!("e2e_smoke task started");
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
        let _ = AIRSPEED_PUB.publish(&sample);
        YieldOnce::new().await;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn e2e_smoke_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
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
            info!("stop is a no-op in this smoke module");
            0
        }
        _ => {
            px4_log::err!("usage: e2e_smoke {{start|stop|status}}");
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
