//! `Timer` — one-shot async sleep on PX4's high-resolution timer.
//!
//! `sleep(duration).await` returns after `duration` has elapsed.
//! The future owns a pinned `hrt_call` slot; on first poll it arms
//! the slot via `hrt_call_after` (or, on the host mock, a worker
//! thread). The HRT callback flips a flag and wakes the registered
//! waker. The runtime polls the future again, sees the flag, and
//! resolves.
//!
//! Cancellation: dropping a `Sleep` before it fires runs `hrt_cancel`
//! to remove the entry from the timer queue, so the storage stays
//! valid until PX4 stops touching it. The `Drop` is the reason
//! `Sleep` is `!Unpin`.
//!
//! ```ignore
//! use core::time::Duration;
//! use px4_workqueue::{sleep, task};
//!
//! #[task(wq = "rate_ctrl")]
//! async fn one_hz() {
//!     loop {
//!         do_work();
//!         sleep(Duration::from_secs(1)).await;
//!     }
//! }
//! ```

use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::future::Future;
use core::marker::PhantomPinned;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::{Context, Poll};
use core::time::Duration;

use crate::AtomicWaker;
use crate::hrt;

const STATE_IDLE: u8 = 0;
const STATE_ARMED: u8 = 1;
const STATE_FIRED: u8 = 2;

/// Future returned by [`sleep`].
///
/// Stored as a local in an `async fn` (which becomes a field of the
/// pinned task future). The HRT callback receives `&self.fired` and
/// `&self.waker` by pointer; the type is `!Unpin` so those addresses
/// stay valid until `Drop` cancels the timer.
pub struct Sleep {
    delay_us: u64,
    state: AtomicU8,
    waker: AtomicWaker,
    hrt: UnsafeCell<MaybeUninit<hrt::HrtCall>>,
    _pin: PhantomPinned,
}

// SAFETY: `state`, `waker` and `hrt` are accessed only from the
// owning task's WQ thread (poll path) or from the HRT callback.
// PX4's HRT serialises callbacks, and the state machine ensures the
// callback observes a fully-constructed `Sleep` before firing.
unsafe impl Send for Sleep {}
unsafe impl Sync for Sleep {}

impl Sleep {
    fn new(duration: Duration) -> Self {
        // hrt_abstime is microseconds. Saturate on overflow so a
        // ridiculous Duration still produces a deterministic delay
        // rather than UB.
        let delay_us = duration.as_micros().min(u64::MAX as u128) as u64;
        Self {
            delay_us,
            state: AtomicU8::new(STATE_IDLE),
            waker: AtomicWaker::new(),
            hrt: UnsafeCell::new(MaybeUninit::uninit()),
            _pin: PhantomPinned,
        }
    }
}

/// Build a future that completes after `duration` has elapsed.
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: we never move out of `self`. Pin guarantees the
        // address is stable for the lifetime of the future.
        let this = unsafe { self.get_unchecked_mut() };

        match this.state.load(Ordering::Acquire) {
            STATE_FIRED => return Poll::Ready(()),
            STATE_ARMED => {
                this.waker.register(cx.waker());
                // Re-check after registering to avoid a missed wake.
                if this.state.load(Ordering::Acquire) == STATE_FIRED {
                    return Poll::Ready(());
                }
                return Poll::Pending;
            }
            _ => {}
        }

        // First poll. Arm the timer.
        this.waker.register(cx.waker());
        // SAFETY: no other thread sees the hrt slot before STATE_ARMED
        // is set; the callback we register reads `&this.state` and
        // `&this.waker`, both stable for the lifetime of `Sleep`.
        let hrt_ptr = unsafe { (*this.hrt.get()).as_mut_ptr() };
        let ctx = this as *const Sleep as *mut c_void;
        // SAFETY: hrt_call_after takes ownership of the entry; we
        // give it a pointer into our pinned storage and never move
        // until Drop runs hrt_cancel.
        unsafe {
            hrt::call_after(hrt_ptr, this.delay_us, sleep_callout, ctx);
        }
        this.state.store(STATE_ARMED, Ordering::Release);
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        // Cancel the timer before letting the storage go away.
        // `hrt_cancel` is a no-op if the entry already fired or was
        // never armed (PX4 tracks state internally).
        if self.state.load(Ordering::Acquire) != STATE_IDLE {
            // SAFETY: hrt was initialised when STATE_ARMED was set.
            let hrt_ptr = unsafe { (*self.hrt.get()).as_mut_ptr() };
            unsafe { hrt::cancel(hrt_ptr) };
        }
    }
}

/// HRT callback — runs in PX4's HRT thread context. Keep it short:
/// flip the state, wake the task, return.
unsafe extern "C" fn sleep_callout(ctx: *mut c_void) {
    // SAFETY: ctx is a `&Sleep` set by `Sleep::poll`. Its address is
    // stable until `Drop`, and `Drop` runs `hrt_cancel` first, which
    // synchronises against any in-flight callback in PX4's HRT.
    let this = unsafe { &*(ctx as *const Sleep) };
    this.state.store(STATE_FIRED, Ordering::Release);
    this.waker.wake();
}
