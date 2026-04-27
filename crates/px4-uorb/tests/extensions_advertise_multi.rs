//! Phase 06.6 — `Publication::advertise_multi` returns the assigned
//! instance and a subsequent publish reaches a subscriber.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

mod common;
use common::{SensorGyro, sample, sensor_gyro, yield_now};

use core::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use px4_uorb::{Publication, Subscription};
use px4_workqueue::{drain_until_idle, wq_configurations};

static PUB: Publication<sensor_gyro> = Publication::new();
static RECEIVED: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicBool = AtomicBool::new(false);

type SubFut = impl Future<Output = ()>;
static SUB: px4_workqueue::WorkItemCell<SubFut> = px4_workqueue::WorkItemCell::new();

type PubFut = impl Future<Output = ()>;
static PROD: px4_workqueue::WorkItemCell<PubFut> = px4_workqueue::WorkItemCell::new();

#[define_opaque(SubFut)]
fn make_sub() -> SubFut {
    async {
        let s = Subscription::<sensor_gyro>::new();
        for _ in 0..3 {
            let m: SensorGyro = s.recv().await;
            RECEIVED.fetch_add(m.device_id, Ordering::AcqRel);
        }
        DONE.store(true, Ordering::Release);
    }
}

#[define_opaque(PubFut)]
fn make_pub() -> PubFut {
    async {
        // Eagerly advertise on instance 7 with an initial sample.
        let assigned = PUB.advertise_multi(&sample(0), 7);
        // Mock returns whatever we asked for; the substantive check
        // is that the call goes through and gives back a real i32.
        assert_eq!(assigned, 7);
        for i in 1..=3u32 {
            PUB.publish(&sample(i)).expect("publish");
            yield_now().await;
        }
    }
}

#[test]
fn advertise_multi_returns_instance_and_publishes() {
    px4_uorb::_reset_broker();
    SUB.spawn(make_sub(), &wq_configurations::test1, c"adv_sub")
        .forget();
    PROD.spawn(make_pub(), &wq_configurations::test1, c"adv_pub")
        .forget();

    for _ in 0..50 {
        if DONE.load(Ordering::Acquire) {
            break;
        }
        drain_until_idle();
    }

    assert!(DONE.load(Ordering::Acquire), "subscriber did not finish");
    // 1 + 2 + 3 = 6.
    assert_eq!(RECEIVED.load(Ordering::Acquire), 6);
}
