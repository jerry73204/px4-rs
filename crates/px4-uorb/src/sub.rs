//! Typed `Subscription<T>` + async `recv()`.

use core::cell::Cell;
use core::ffi::c_void;
use core::future::Future;
use core::marker::{PhantomData, PhantomPinned};
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::task::{Context, Poll};

use px4_workqueue::AtomicWaker;

use crate::ffi;
use crate::topic::UorbTopic;

/// Typed subscriber.
///
/// `Subscription<T>` is `!Send`: the underlying `SubscriptionCallback`
/// remembers the address of `self.waker`, so the value must not move
/// after the first `recv()` poll. Holding it in an `async fn` local —
/// which becomes a field of the pinned future state inside its
/// `WorkItemCell` — is the supported pattern.
pub struct Subscription<T: UorbTopic> {
    cb: Cell<*mut ffi::SubCb>,
    waker: AtomicWaker,
    interval_us: u32,
    instance: u8,
    _t: PhantomData<fn() -> T>,
    /// `*const ()` is `!Send` and `!Sync`.
    _not_send: PhantomData<*const ()>,
    _pin: PhantomPinned,
}

impl<T: UorbTopic> Subscription<T> {
    pub const fn new() -> Self {
        Self::new_with(0, 0)
    }

    /// Construct a subscription with an explicit minimum interval
    /// between deliveries. PX4's `SubscriptionCallback` enforces this
    /// by skipping `call()` invocations that arrive sooner than
    /// `interval_us` microseconds after the previous one — useful for
    /// throttling a high-rate publisher.
    pub const fn with_interval_us(interval_us: u32) -> Self {
        Self::new_with(interval_us, 0)
    }

    /// Construct a subscription on a specific multi-instance index.
    /// Defaults to 0; use this when a topic has multiple advertised
    /// instances and you need a particular one.
    pub const fn with_instance(instance: u8) -> Self {
        Self::new_with(0, instance)
    }

    /// Combined-knobs constructor.
    pub const fn new_with(interval_us: u32, instance: u8) -> Self {
        Self {
            cb: Cell::new(core::ptr::null_mut()),
            waker: AtomicWaker::new(),
            interval_us,
            instance,
            _t: PhantomData,
            _not_send: PhantomData,
            _pin: PhantomPinned,
        }
    }

    /// Lazily construct the underlying `SubscriptionCallback` and
    /// register it with PX4. Idempotent.
    fn ensure_registered(&self) {
        if !self.cb.get().is_null() {
            return;
        }
        // SAFETY: `&self.waker` is stable for the lifetime of `self`.
        // Callers ensure `self` doesn't move after this point (see the
        // type-level `_pin` and `!Send` markers + the doc note).
        let cb = unsafe {
            ffi::sub_cb_new(
                T::metadata(),
                self.interval_us,
                self.instance,
                &self.waker as *const _ as *mut c_void,
                wake_trampoline,
            )
        };
        unsafe {
            ffi::sub_cb_register(cb);
        }
        self.cb.set(cb);
    }

    /// Non-blocking poll. Returns `Some` if a new message has arrived
    /// since the last `try_recv` / `recv` on this subscription.
    pub fn try_recv(&self) -> Option<T::Msg> {
        self.ensure_registered();
        let cb = self.cb.get();
        if cb.is_null() {
            return None;
        }
        let mut buf: MaybeUninit<T::Msg> = MaybeUninit::uninit();
        // SAFETY: `T::Msg` is `#[repr(C)]` POD and the metadata's
        // `o_size` matches it (verified at codegen time).
        let updated = unsafe { ffi::sub_cb_update(cb, buf.as_mut_ptr() as *mut c_void) };
        if updated {
            // SAFETY: `updated == true` means PX4 wrote `o_size` bytes.
            Some(unsafe { buf.assume_init() })
        } else {
            None
        }
    }

    /// Async wait for the next message.
    pub fn recv(&self) -> Recv<'_, T> {
        Recv { sub: self }
    }
}

impl<T: UorbTopic> Default for Subscription<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: UorbTopic> Drop for Subscription<T> {
    fn drop(&mut self) {
        let cb = self.cb.replace(core::ptr::null_mut());
        if !cb.is_null() {
            // SAFETY: we own cb; sub_cb_delete unregisters first.
            unsafe { ffi::sub_cb_delete(cb) };
        }
    }
}

/// Future returned by `Subscription::recv`.
pub struct Recv<'a, T: UorbTopic> {
    sub: &'a Subscription<T>,
}

impl<'a, T: UorbTopic> Future for Recv<'a, T> {
    type Output = T::Msg;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T::Msg> {
        // Fast path: a sample is already pending.
        if let Some(m) = self.sub.try_recv() {
            return Poll::Ready(m);
        }
        self.sub.waker.register(cx.waker());
        // Re-check after registering to avoid a missed wake.
        if let Some(m) = self.sub.try_recv() {
            return Poll::Ready(m);
        }
        Poll::Pending
    }
}

unsafe extern "C" fn wake_trampoline(ctx: *mut c_void) {
    // SAFETY: ctx was set in `sub_cb_new` to `&self.waker`.
    let waker = unsafe { &*(ctx as *const AtomicWaker) };
    waker.wake();
}
