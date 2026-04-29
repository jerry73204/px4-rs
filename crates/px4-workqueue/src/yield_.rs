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
//!
//! **NuttX-specific quirk.** On NuttX, `ScheduleNow` `sem_post`s the
//! WorkQueue thread's wakeup semaphore and returns. The same WQ
//! thread that just finished polling will see a non-empty queue and
//! immediately pick the work item back up — `WorkQueue::Run()`'s
//! inner `while (!_q.empty())` drains all queued work in one batch,
//! never re-entering `sem_wait` between items. With a self-rescheduling
//! task that's the only thing in the queue, the WQ thread becomes a
//! tight loop that never crosses a kernel scheduling boundary.
//! `sched_yield` doesn't help — it only redistributes the CPU among
//! same-priority runnable threads, and there are none. Higher-priority
//! threads (`nsh`) DO preempt at clock ticks, but only if they're
//! runnable: if `nsh` is blocked waiting for the child task that
//! ran the `start` command to exit, and that child is queued behind
//! the WQ thread for `printf` access to the console, the whole
//! shell hangs.
//!
//! Fix: do an actual `usleep(1)` instead of `sched_yield`. A 1-µs
//! sleep forces NuttX to put the calling thread on the timed-wait
//! list and run the scheduler — at which point IDLE / nsh / any
//! other ready thread gets the CPU. On POSIX SITL `usleep(1)` is a
//! near-noop; the lockstep scheduler that drives SITL doesn't care
//! either way. Gated `cfg(target_os = "none")` so host-mock tests
//! (cargo test) skip the libc call cleanly.

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
        // See module-level note: on NuttX the WQ thread drains its
        // queue in a tight loop without ever crossing a kernel
        // scheduling boundary, so sched_yield is a noop and the
        // shell hangs. usleep(1) puts us on the timed-wait list and
        // forces NuttX to run the scheduler, which then picks up nsh
        // and any other ready higher-priority work.
        #[cfg(target_os = "none")]
        unsafe {
            usleep(1);
        }
        Poll::Pending
    }
}

// NuttX libc provides `usleep(3)`. Linked into the firmware alongside
// the rest of newlib + NuttX's libc; no extra build-system plumbing
// needed.
#[cfg(target_os = "none")]
unsafe extern "C" {
    fn usleep(usec: core::ffi::c_uint) -> core::ffi::c_int;
}
