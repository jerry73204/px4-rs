//! Phase 06.6 — `Subscription::with_interval_us` constructor smoke
//! test. The host mock collapses every interval onto "deliver
//! immediately", so the substantive thing this verifies is that the
//! FFI signature still lines up — a target build that wires the
//! parameter into PX4's `SubscriptionCallback` is exercised by the
//! SITL test crate.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

mod common;
use common::{SensorGyro, sensor_gyro};

use px4_uorb::Subscription;
use px4_workqueue::{drain_until_idle, task};

#[task(wq = "test1")]
async fn smoke() {
    let s = Subscription::<sensor_gyro>::with_interval_us(10_000);
    let _: Option<SensorGyro> = s.try_recv();
}

#[test]
fn with_interval_us_constructor_compiles_and_runs() {
    px4_uorb::_reset_broker();
    smoke::spawn().forget();
    drain_until_idle();
}
