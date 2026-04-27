//! `Notify` — single-waiter async signal.
//!
//! One task awaits `notify.notified().await`; another (or several)
//! call `notify.notify()` to wake it. A `notify` that arrives before
//! a `notified()` is registered stores a permit, so the next
//! `notified().await` returns immediately. Multiple notifies coalesce
//! — no count is accumulated.
//!
//! Modeled on `tokio::sync::Notify::notify_one`, but trimmed to one
//! waker slot (`AtomicWaker`) since the runtime guarantees one
//! pollable task per `WorkItemCell`. If you need multi-waiter
//! signalling, use `Channel<(), N>` instead.
//!
//! ```ignore
//! use px4_workqueue::Notify;
//!
//! static SIGNAL: Notify = Notify::new();
//!
//! #[task(wq = "rate_ctrl")]
//! async fn waiter() {
//!     loop {
//!         SIGNAL.notified().await;
//!         do_thing();
//!     }
//! }
//!
//! #[task(wq = "lp_default")]
//! async fn poker() {
//!     loop {
//!         SIGNAL.notify();
//!         sleep(Duration::from_millis(100)).await;
//!     }
//! }
//! ```

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll};

use crate::AtomicWaker;

/// Edge-triggered signal between async tasks.
pub struct Notify {
    permit: AtomicBool,
    waker: AtomicWaker,
}

impl Notify {
    /// Construct a `Notify` with no permit and no waiter.
    pub const fn new() -> Self {
        Self {
            permit: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }

    /// Wake the waiter, or store a permit for the next `notified()`.
    /// Re-calling this with the permit already set is a no-op.
    pub fn notify(&self) {
        self.permit.store(true, Ordering::Release);
        self.waker.wake();
    }

    /// Return a future that completes the next time `notify()` runs
    /// (or immediately, if a permit is already stored).
    pub fn notified(&self) -> Notified<'_> {
        Notified { notify: self }
    }
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by [`Notify::notified`]. Holds a borrow of the
/// parent `Notify` so multiple producers can keep notifying through
/// the same handle.
pub struct Notified<'a> {
    notify: &'a Notify,
}

impl<'a> Future for Notified<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // Fast path: a permit is already waiting. `swap` consumes it.
        if self.notify.permit.swap(false, Ordering::AcqRel) {
            return Poll::Ready(());
        }
        self.notify.waker.register(cx.waker());
        // Re-check after registering to avoid a missed wake (a notify
        // that landed between the first swap and the register).
        if self.notify.permit.swap(false, Ordering::AcqRel) {
            return Poll::Ready(());
        }
        Poll::Pending
    }
}
