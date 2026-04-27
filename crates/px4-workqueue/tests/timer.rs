//! Host-mock tests for `Sleep`.
//!
//! On the host, the HRT mock spawns a short-lived thread that sleeps
//! for the requested duration and then invokes the callout. These
//! tests use small deltas so the suite stays fast.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

use core::future::Future;
use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;
use std::time::Instant;

use px4_workqueue::{WorkItemCell, drain_until_idle, sleep, wq_configurations};

mod single_sleep_completes {
    use super::*;

    static FIRED: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            sleep(Duration::from_millis(50)).await;
            FIRED.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn test() {
        let started = Instant::now();
        CELL.spawn(make(), &wq_configurations::test1, c"sleep_once")
            .forget();

        // Poll-pump until the sleep fires. The drain helper sleeps a
        // short fixed beat each iteration, so we cap the loop at
        // something well above the sleep duration to avoid hanging
        // a broken implementation.
        for _ in 0..50 {
            if FIRED.load(Ordering::Acquire) > 0 {
                break;
            }
            drain_until_idle();
        }

        assert_eq!(
            FIRED.load(Ordering::Acquire),
            1,
            "Sleep::poll never resolved within the timeout"
        );
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "Sleep resolved before the requested 50ms had elapsed: {:?}",
            started.elapsed()
        );
    }
}

mod sequential_sleeps_accumulate {
    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            for _ in 0..3 {
                sleep(Duration::from_millis(20)).await;
                COUNTER.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    #[test]
    fn test() {
        let started = Instant::now();
        CELL.spawn(make(), &wq_configurations::test1, c"sleep_seq")
            .forget();

        for _ in 0..100 {
            if COUNTER.load(Ordering::Acquire) >= 3 {
                break;
            }
            drain_until_idle();
        }

        assert_eq!(COUNTER.load(Ordering::Acquire), 3);
        // 3 × 20ms = 60ms minimum.
        assert!(
            started.elapsed() >= Duration::from_millis(60),
            "three sleeps should add up to at least 60ms, got {:?}",
            started.elapsed()
        );
    }
}
