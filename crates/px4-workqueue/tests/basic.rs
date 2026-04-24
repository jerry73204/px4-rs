//! End-to-end tests of the runtime (host mock).
//!
//! Each test owns its own statics (state + cell) so the default parallel
//! test harness doesn't leak counter updates across tests.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::task::{Context, Poll};

use px4_workqueue::{AtomicWaker, SpawnError, WorkItemCell, drain_until_idle, wq_configurations};

struct WaitOnce {
    fired: &'static AtomicBool,
    waker: &'static AtomicWaker,
    registered: bool,
}

impl Future for WaitOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.fired.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        if !self.registered {
            self.waker.register(cx.waker());
            self.registered = true;
            if self.fired.load(Ordering::Acquire) {
                return Poll::Ready(());
            }
        }
        Poll::Pending
    }
}

mod spawn_poll_ready {
    use super::*;

    static FIRED: AtomicBool = AtomicBool::new(false);
    static WAKER: AtomicWaker = AtomicWaker::new();
    static RUNS: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            RUNS.fetch_add(1, Ordering::AcqRel);
            WaitOnce {
                fired: &FIRED,
                waker: &WAKER,
                registered: false,
            }
            .await;
            RUNS.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn test() {
        CELL.spawn(make(), &wq_configurations::test1, c"spawn_poll_ready")
            .forget();

        drain_until_idle();
        assert_eq!(
            RUNS.load(Ordering::Acquire),
            1,
            "future should have yielded once at WaitOnce"
        );

        FIRED.store(true, Ordering::Release);
        WAKER.wake();

        drain_until_idle();
        assert_eq!(
            RUNS.load(Ordering::Acquire),
            2,
            "future should have resumed and completed"
        );
    }
}

mod double_spawn_returns_busy {
    use super::*;

    // Waker that never fires: the future stays pending forever.
    static FIRED: AtomicBool = AtomicBool::new(false);
    static WAKER: AtomicWaker = AtomicWaker::new();

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            WaitOnce {
                fired: &FIRED,
                waker: &WAKER,
                registered: false,
            }
            .await;
        }
    }

    #[test]
    fn test() {
        CELL.try_spawn(make(), &wq_configurations::test1, c"busy1")
            .expect("first spawn")
            .forget();

        drain_until_idle(); // let the first poll run so state is stably SPAWNED

        let err = CELL
            .try_spawn(make(), &wq_configurations::test1, c"busy2")
            .expect_err("second spawn should fail");
        assert_eq!(err, SpawnError::Busy);
    }
}

mod respawn_after_finish {
    use super::*;

    static FIRED: AtomicBool = AtomicBool::new(false);
    static WAKER: AtomicWaker = AtomicWaker::new();
    static CYCLES: AtomicU32 = AtomicU32::new(0);

    type Fut = impl Future<Output = ()>;
    static CELL: WorkItemCell<Fut> = WorkItemCell::new();

    #[define_opaque(Fut)]
    fn make() -> Fut {
        async {
            // Wait once, then complete.
            WaitOnce {
                fired: &FIRED,
                waker: &WAKER,
                registered: false,
            }
            .await;
            CYCLES.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn test() {
        CELL.spawn(make(), &wq_configurations::test1, c"respawn")
            .forget();

        drain_until_idle();
        FIRED.store(true, Ordering::Release);
        WAKER.wake();
        drain_until_idle();
        assert_eq!(CYCLES.load(Ordering::Acquire), 1);

        // Reset for the second run.
        FIRED.store(false, Ordering::Release);

        // Respawn should now succeed.
        CELL.try_spawn(make(), &wq_configurations::test1, c"respawn")
            .expect("respawn after finish")
            .forget();

        drain_until_idle();
        FIRED.store(true, Ordering::Release);
        WAKER.wake();
        drain_until_idle();
        assert_eq!(CYCLES.load(Ordering::Acquire), 2);
    }
}
