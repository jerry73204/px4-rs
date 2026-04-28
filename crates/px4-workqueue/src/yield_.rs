//! `yield_now()` — co-operative one-poll yield for `#[task]` async fns.
//!
//! PX4's WorkQueue is C++ scheduling infrastructure unaware of Rust
//! futures, so there's no native primitive that says "park me, run a
//! sibling, resume". The standard async pattern fills that gap with a
//! one-shot future that registers its waker, returns `Pending`, and
//! resolves to `Ready` on the next poll. Tokio and async-std both
//! ship the same shape under `task::yield_now`.
//!
//! Use it whenever a tight `loop { do_thing(); }` would otherwise
//! starve the WorkQueue thread:
//!
//! ```ignore
//! #[task(wq = "lp_default")]
//! async fn pump() {
//!     loop {
//!         publish();
//!         yield_now().await;
//!     }
//! }
//! ```
//!
//! The waker the future registers is the task's own — `wake_by_ref`
//! triggers `ScheduleNow`, which re-queues the WorkItem behind any
//! sibling work that became ready in the meantime.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Yield once. The first `poll` registers the calling task's waker
/// for re-schedule and returns `Pending`; the next `poll` returns
/// `Ready`.
pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

/// Future returned by [`yield_now`]. `Copy` would be a footgun on a
/// future, so we leave it move-only — same shape `tokio` uses.
#[must_use = "futures do nothing unless awaited"]
pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            return Poll::Ready(());
        }
        self.yielded = true;
        // Schedule another poll without consuming the waker.
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
