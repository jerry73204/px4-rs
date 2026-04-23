//! End-to-end test of `#[task]` against the host mock runtime.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::task::{Context, Poll};

use px4_workqueue::{AtomicWaker, drain_until_idle, task};

static FIRED: AtomicBool = AtomicBool::new(false);
static WAKER: AtomicWaker = AtomicWaker::new();
static RUNS: AtomicU32 = AtomicU32::new(0);

struct WaitOnce {
    registered: bool,
}

impl Future for WaitOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if FIRED.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        if !self.registered {
            WAKER.register(cx.waker());
            self.registered = true;
            if FIRED.load(Ordering::Acquire) {
                return Poll::Ready(());
            }
        }
        Poll::Pending
    }
}

#[task(wq = "test1")]
async fn rate_watch(bump: u32) {
    RUNS.fetch_add(bump, Ordering::AcqRel);
    WaitOnce { registered: false }.await;
    RUNS.fetch_add(bump, Ordering::AcqRel);
}

#[test]
fn task_macro_expands_and_runs() {
    rate_watch::spawn(10).forget();

    drain_until_idle();
    assert_eq!(RUNS.load(Ordering::Acquire), 10);

    FIRED.store(true, Ordering::Release);
    WAKER.wake();

    drain_until_idle();
    assert_eq!(RUNS.load(Ordering::Acquire), 20);
}
