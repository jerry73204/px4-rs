//! Host-mock tests for `Notify`.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

use core::future::Future;
use core::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use px4_workqueue::{Notify, WorkItemCell, drain_until_idle, wq_configurations};

mod notify_wakes_a_waiter {
    use super::*;

    static SIGNAL: Notify = Notify::new();
    static RUNS: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            RUNS.fetch_add(1, Ordering::AcqRel);
            SIGNAL.notified().await;
            RUNS.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn test() {
        CELL.spawn(make(), &wq_configurations::test1, c"notify_basic")
            .forget();

        drain_until_idle();
        assert_eq!(
            RUNS.load(Ordering::Acquire),
            1,
            "task should have parked at notified().await"
        );

        SIGNAL.notify();
        drain_until_idle();
        assert_eq!(
            RUNS.load(Ordering::Acquire),
            2,
            "notify() should have woken the parked task"
        );
    }
}

mod permit_stored_when_notify_precedes_wait {
    use super::*;

    static SIGNAL: Notify = Notify::new();
    static RUNS: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            // notify() ran before this task was even spawned; the
            // permit is already set, so notified().await must
            // resolve on the very first poll without parking.
            SIGNAL.notified().await;
            RUNS.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn test() {
        SIGNAL.notify();
        CELL.spawn(make(), &wq_configurations::test1, c"notify_permit")
            .forget();

        drain_until_idle();
        assert_eq!(
            RUNS.load(Ordering::Acquire),
            1,
            "stored permit should let notified() return immediately"
        );
    }
}

mod multi_notifies_coalesce {
    use super::*;

    static SIGNAL: Notify = Notify::new();
    static WAKES: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            // Three notifies will fire before this loop body runs;
            // they should coalesce into one wake — the second pass
            // through notified().await must park.
            SIGNAL.notified().await;
            WAKES.fetch_add(1, Ordering::AcqRel);
            SIGNAL.notified().await;
            WAKES.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn test() {
        SIGNAL.notify();
        SIGNAL.notify();
        SIGNAL.notify();
        CELL.spawn(make(), &wq_configurations::test1, c"notify_coalesce")
            .forget();

        // First notified() consumes the (single) permit; second
        // notified() must park because no further notify has run yet.
        drain_until_idle();
        assert_eq!(
            WAKES.load(Ordering::Acquire),
            1,
            "three pre-notifies should coalesce into one permit"
        );

        // Now wake it.
        thread::sleep(Duration::from_millis(5));
        SIGNAL.notify();
        drain_until_idle();
        assert_eq!(
            WAKES.load(Ordering::Acquire),
            2,
            "post-spawn notify should resume the parked task"
        );
    }
}
