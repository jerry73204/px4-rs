//! `e2e_pubsub_pub` — phase-11 pub/sub publisher half.
//!
//! Publishes an incrementing `E2ePubsub` topic on `lp_default`. Used
//! together with `e2e_pubsub_sub` (work item 11.7) to verify the
//! `Subscription` path through PX4's real broker.

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

module!("e2e_pubsub_pub");
panic_handler!();

#[px4_message("E2ePubsub.msg")]
pub struct E2ePubsub;

static PUB: Publication<e2e_pubsub> = Publication::new();

#[task(wq = "lp_default")]
async fn pump() {
    info!("e2e_pubsub_pub task started");
    let mut counter: u32 = 0;
    loop {
        counter = counter.wrapping_add(1);
        let sample = E2ePubsub {
            timestamp: counter as u64,
            counter,
            _padding0: [0; 4],
        };
        let _ = PUB.publish(&sample);
        YieldOnce::new().await;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn e2e_pubsub_pub_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
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
            info!("stop is a no-op in this test module");
            0
        }
        _ => {
            px4_log::err!("usage: e2e_pubsub_pub {{start|stop|status}}");
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
