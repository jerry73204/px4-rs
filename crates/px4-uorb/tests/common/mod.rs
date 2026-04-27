//! Shared fixtures for the phase-06.6 extension tests. Each test
//! file lives in its own binary, so we factor the message + helpers
//! here and `mod common` from each.

#![allow(dead_code)]

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use px4_msg_macros::px4_message;

#[px4_message("tests/fixtures/SensorGyro.msg")]
pub struct SensorGyro;

pub fn sample(stamp: u32) -> SensorGyro {
    SensorGyro {
        timestamp: stamp as u64,
        timestamp_sample: stamp as u64,
        device_id: stamp,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        temperature: 0.0,
        error_count: 0,
        clip_counter: [0; 3],
        samples: 0,
        _padding0: [0; 4],
    }
}

pub struct YieldOnce(bool);
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
pub fn yield_now() -> YieldOnce {
    YieldOnce(false)
}
