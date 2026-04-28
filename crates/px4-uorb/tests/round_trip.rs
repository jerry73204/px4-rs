//! Pub/sub round-trip via `#[task]` against the host mock.
//!
//! Spawns one publisher task and one subscriber task. The publisher
//! emits N samples; the subscriber `recv().await`s them one at a time.
//! Verifies the count and that the last payload matches.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use px4_msg_macros::px4_message;
use px4_uorb::{Publication, Subscription};
use px4_workqueue::{drain_until_idle, task, yield_now};

#[px4_message("tests/fixtures/SensorGyro.msg")]
pub struct SensorGyro;

const N: u32 = 1000;

static PUB: Publication<sensor_gyro> = Publication::new();

static RECEIVED: AtomicU32 = AtomicU32::new(0);
static LAST_DEVICE_ID: AtomicU32 = AtomicU32::new(0);
static SUB_DONE: AtomicBool = AtomicBool::new(false);

#[task(wq = "test1")]
async fn subscriber() {
    let sub = Subscription::<sensor_gyro>::new();
    while RECEIVED.load(Ordering::Acquire) < N {
        let msg: SensorGyro = sub.recv().await;
        LAST_DEVICE_ID.store(msg.device_id, Ordering::Release);
        RECEIVED.fetch_add(1, Ordering::AcqRel);
    }
    SUB_DONE.store(true, Ordering::Release);
}

#[task(wq = "test1")]
async fn publisher() {
    // Both tasks share `test1`'s WQ so `yield_now` reliably hands
    // control to the subscriber between publishes.
    for i in 1..=N {
        let s = SensorGyro {
            timestamp: i as u64,
            timestamp_sample: i as u64,
            device_id: i,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            temperature: 0.0,
            error_count: 0,
            clip_counter: [0; 3],
            samples: 0,
            _padding0: [0; 4],
        };
        PUB.publish(&s).expect("publish");
        // Hand the WQ to the subscriber so it consumes this sample
        // before the next publish overwrites the broker's slot.
        yield_now().await;
    }
}

#[test]
fn publisher_to_subscriber_round_trip() {
    px4_uorb::_reset_broker();

    subscriber::spawn().forget();
    publisher::spawn().forget();

    // Mock dispatcher needs a beat to drain N publishes through the
    // wake path. With N=50 this completes in a few ms.
    for _ in 0..200 {
        if SUB_DONE.load(Ordering::Acquire) {
            break;
        }
        drain_until_idle();
    }

    assert!(
        SUB_DONE.load(Ordering::Acquire),
        "subscriber didn't finish; received {} of {N}",
        RECEIVED.load(Ordering::Acquire),
    );
    assert_eq!(RECEIVED.load(Ordering::Acquire), N);
    assert_eq!(LAST_DEVICE_ID.load(Ordering::Acquire), N);
}
