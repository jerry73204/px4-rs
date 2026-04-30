//! Byte-shaped publication / subscription wrappers — type-erased
//! counterparts of [`Publication<T>`](crate::Publication) and
//! [`Subscription<T>`](crate::Subscription).
//!
//! Where the typed variants take a compile-time `T: UorbTopic` and use
//! `T::metadata()` + `T::Msg` for the FFI bridge, the raw variants
//! take a runtime `&'static orb_metadata` and operate on `&[u8]` /
//! `&mut [u8]`. Useful for crates that need to expose a backend-style
//! byte interface (e.g. `nros-rmw-uorb`'s `Session` impl, where
//! callers pass arbitrary metadata pointers without compile-time type
//! erasure).
//!
//! Both branches (real PX4 FFI on target, std mock on host) share the
//! same wrapper implementation here — the underlying `crate::ffi`
//! module routes to the right backend.
//!
//! Layout discipline mirrors [`Publication`](crate::Publication) /
//! [`Subscription`](crate::Subscription):
//!
//! - [`RawPublication`] uses an `AtomicPtr<c_void>` for its lazy
//!   advertise handle — `Send + Sync`, can live in `static`.
//! - [`RawSubscription`] is `!Send + !Sync` and pins itself once
//!   `try_recv` is first called, because PX4's
//!   `SubscriptionCallback` records the address of the embedded
//!   `AtomicWaker` as its callback context.

use core::cell::Cell;
use core::ffi::c_void;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicPtr, Ordering};

use px4_sys::orb_metadata;
use px4_workqueue::AtomicWaker;

use crate::ffi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawPubError {
    /// `orb_publish` returned a non-zero status.
    Failed,
    /// Caller-provided byte slice did not match `metadata.o_size`.
    SizeMismatch,
}

/// Type-erased publisher. Construction lazily calls
/// `orb_advertise_multi` on first `publish` so the same
/// `RawPublication` can live in `static`-style code without doing
/// FFI at startup.
pub struct RawPublication {
    handle: AtomicPtr<c_void>,
    metadata: &'static orb_metadata,
}

// SAFETY: orb_advert_t handles are documented by PX4 as globally
// shareable across threads ("Advertiser handles are global").
unsafe impl Send for RawPublication {}
unsafe impl Sync for RawPublication {}

impl RawPublication {
    /// Construct a `RawPublication` for `metadata`. Does not advertise
    /// yet — the first `publish` call lazily advertises.
    pub const fn new(metadata: &'static orb_metadata) -> Self {
        Self {
            handle: AtomicPtr::new(core::ptr::null_mut()),
            metadata,
        }
    }

    /// Eagerly advertise on the requested instance with an initial
    /// sample (whose byte length must equal `metadata.o_size`).
    /// Returns the instance PX4 actually assigned.
    ///
    /// Returns `Err(RawPubError::SizeMismatch)` if `initial.len()` is
    /// wrong; returns the assigned instance on success.
    pub fn advertise_multi(
        &self,
        initial: &[u8],
        requested_instance: i32,
    ) -> Result<i32, RawPubError> {
        if initial.len() != self.metadata.o_size as usize {
            return Err(RawPubError::SizeMismatch);
        }
        if self.handle.load(Ordering::Acquire).is_null() {
            let mut instance = requested_instance;
            // SAFETY: metadata is 'static; initial points at o_size
            // bytes (just verified).
            let h = unsafe {
                ffi::advertise_multi(self.metadata, initial.as_ptr() as *const c_void, &mut instance)
            };
            if self
                .handle
                .compare_exchange(core::ptr::null_mut(), h, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                // SAFETY: we just allocated h via advertise_multi.
                unsafe { ffi::unadvertise(h) };
            }
            return Ok(instance);
        }
        // Already advertised; mirror Publication::advertise_multi.
        Ok(requested_instance)
    }

    /// Explicitly unadvertise. Subsequent `publish` lazily
    /// re-advertises on instance 0.
    pub fn unadvertise(&self) {
        let h = self.handle.swap(core::ptr::null_mut(), Ordering::AcqRel);
        if !h.is_null() {
            // SAFETY: we owned h until the swap.
            unsafe { ffi::unadvertise(h) };
        }
    }

    fn ensure_advertised(&self, initial: *const c_void) {
        if self.handle.load(Ordering::Acquire).is_null() {
            // SAFETY: metadata is 'static; initial points at o_size bytes.
            let h = unsafe { ffi::advertise(self.metadata, initial) };
            if self
                .handle
                .compare_exchange(core::ptr::null_mut(), h, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                // SAFETY: we just allocated h.
                unsafe { ffi::unadvertise(h) };
            }
        }
    }

    /// Publish one sample. `data.len()` must equal `metadata.o_size`.
    pub fn publish(&self, data: &[u8]) -> Result<(), RawPubError> {
        if data.len() != self.metadata.o_size as usize {
            return Err(RawPubError::SizeMismatch);
        }
        self.ensure_advertised(data.as_ptr() as *const c_void);
        let handle = self.handle.load(Ordering::Acquire);
        // SAFETY: handle is non-null after ensure_advertised; data.len()
        // matches metadata.o_size.
        let rc =
            unsafe { ffi::publish(self.metadata, handle, data.as_ptr() as *const c_void) };
        if rc == 0 {
            Ok(())
        } else {
            Err(RawPubError::Failed)
        }
    }

    /// Borrow the metadata this publisher was constructed with.
    pub fn metadata(&self) -> &'static orb_metadata {
        self.metadata
    }
}

impl Drop for RawPublication {
    fn drop(&mut self) {
        let h = self.handle.swap(core::ptr::null_mut(), Ordering::AcqRel);
        if !h.is_null() {
            // SAFETY: we own h.
            unsafe { ffi::unadvertise(h) };
        }
    }
}

/// Type-erased subscriber. Mirrors [`Subscription<T>`](crate::Subscription)
/// in shape and pinning constraints — see that doc for rationale.
pub struct RawSubscription {
    cb: Cell<*mut ffi::SubCb>,
    waker: AtomicWaker,
    metadata: &'static orb_metadata,
    interval_us: u32,
    instance: u8,
    /// `*const ()` is `!Send + !Sync`.
    _not_send: PhantomData<*const ()>,
}

impl RawSubscription {
    /// Construct a subscriber on instance 0 with no rate limiting.
    pub const fn new(metadata: &'static orb_metadata) -> Self {
        Self::new_with(metadata, 0, 0)
    }

    /// Construct on a specific multi-instance index.
    pub const fn with_instance(metadata: &'static orb_metadata, instance: u8) -> Self {
        Self::new_with(metadata, 0, instance)
    }

    /// Construct with rate limit (microseconds between deliveries) +
    /// instance.
    pub const fn new_with(
        metadata: &'static orb_metadata,
        interval_us: u32,
        instance: u8,
    ) -> Self {
        Self {
            cb: Cell::new(core::ptr::null_mut()),
            waker: AtomicWaker::new(),
            metadata,
            interval_us,
            instance,
            _not_send: PhantomData,
        }
    }

    /// Lazily wire the underlying `SubscriptionCallback`. Idempotent.
    fn ensure_registered(&self) {
        if !self.cb.get().is_null() {
            return;
        }
        // SAFETY: `&self.waker` is stable for the lifetime of `self`.
        // Caller ensures `self` is not moved after first
        // `try_recv` / `register_waker` (mirrors Subscription<T>).
        let cb = unsafe {
            ffi::sub_cb_new(
                self.metadata,
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

    /// Non-blocking byte-shaped poll. On success, copies the latest
    /// sample into `buf` (which must be at least `metadata.o_size`
    /// long) and returns `Some(metadata.o_size)`.
    pub fn try_recv(&self, buf: &mut [u8]) -> Option<usize> {
        let size = self.metadata.o_size as usize;
        if buf.len() < size {
            return None;
        }
        self.ensure_registered();
        let cb = self.cb.get();
        if cb.is_null() {
            return None;
        }
        let updated = unsafe { ffi::sub_cb_update(cb, buf.as_mut_ptr() as *mut c_void) };
        if updated { Some(size) } else { None }
    }

    /// Register a waker on this subscriber's `AtomicWaker`. Wires the
    /// underlying callback if not already registered. Idempotent.
    pub fn register_waker(&self, w: &core::task::Waker) {
        self.ensure_registered();
        self.waker.register(w);
    }

    /// Borrow the metadata this subscriber was constructed with.
    pub fn metadata(&self) -> &'static orb_metadata {
        self.metadata
    }
}

impl Drop for RawSubscription {
    fn drop(&mut self) {
        let cb = self.cb.replace(core::ptr::null_mut());
        if !cb.is_null() {
            // SAFETY: we own cb; sub_cb_delete unregisters first.
            unsafe { ffi::sub_cb_delete(cb) };
        }
    }
}

unsafe extern "C" fn wake_trampoline(ctx: *mut c_void) {
    // SAFETY: ctx was set in `sub_cb_new` to `&self.waker`.
    let waker = unsafe { &*(ctx as *const AtomicWaker) };
    waker.wake();
}
