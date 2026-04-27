//! Phase 06.6 — explicit `Publication::unadvertise` clears the handle
//! so the next publish lazily re-advertises.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

mod common;
use common::{SensorGyro, sample, sensor_gyro};

use core::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};

use px4_uorb::{Publication, Subscription};
use px4_workqueue::{drain_until_idle, wq_configurations};

static PUB: Publication<sensor_gyro> = Publication::new();
static SAW: AtomicU32 = AtomicU32::new(0);

type Fut = impl Future<Output = ()>;
static CELL: px4_workqueue::WorkItemCell<Fut> = px4_workqueue::WorkItemCell::new();

#[define_opaque(Fut)]
fn make() -> Fut {
    async {
        let s = Subscription::<sensor_gyro>::new();
        // Round 1.
        PUB.publish(&sample(11)).expect("publish 1");
        let m: SensorGyro = s.recv().await;
        SAW.fetch_add(m.device_id, Ordering::AcqRel);

        // Drop the advert; the next publish must re-advertise.
        PUB.unadvertise();
        PUB.publish(&sample(22)).expect("publish 2");
        let m: SensorGyro = s.recv().await;
        SAW.fetch_add(m.device_id, Ordering::AcqRel);
    }
}

#[test]
fn unadvertise_then_publish_re_advertises() {
    px4_uorb::_reset_broker();
    CELL.spawn(make(), &wq_configurations::test1, c"unadv")
        .forget();

    for _ in 0..50 {
        if SAW.load(Ordering::Acquire) >= 33 {
            break;
        }
        drain_until_idle();
    }
    assert_eq!(
        SAW.load(Ordering::Acquire),
        33,
        "expected 11 + 22 across the unadvertise boundary"
    );
}
