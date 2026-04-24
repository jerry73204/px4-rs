//! Typed `Publication<T>`.

use core::ffi::c_void;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::ffi;
use crate::topic::UorbTopic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PubError {
    /// `orb_publish` returned a non-zero status.
    Failed,
}

/// Typed publisher for a uORB topic.
///
/// Construction lazily calls `orb_advertise_multi` so a `Publication`
/// can live in `static`-style code without doing FFI at startup.
/// The first `publish()` advertises if needed.
pub struct Publication<T: UorbTopic> {
    handle: AtomicPtr<c_void>,
    _t: PhantomData<fn() -> T>,
}

// SAFETY: orb_advert_t handles are documented by PX4 as globally
// shareable across threads ("Advertiser handles are global").
unsafe impl<T: UorbTopic> Send for Publication<T> {}
unsafe impl<T: UorbTopic> Sync for Publication<T> {}

impl<T: UorbTopic> Publication<T> {
    pub const fn new() -> Self {
        Self {
            handle: AtomicPtr::new(core::ptr::null_mut()),
            _t: PhantomData,
        }
    }

    /// Eagerly advertise with an initial sample. Optional — `publish()`
    /// will advertise lazily if not called.
    pub fn advertise(&self, initial: &T::Msg) {
        self.ensure_advertised(initial as *const _ as *const c_void);
    }

    fn ensure_advertised(&self, initial: *const c_void) {
        if self.handle.load(Ordering::Acquire).is_null() {
            // SAFETY: metadata is a 'static reference; initial points
            // at a stack copy of T::Msg with matching size.
            let h = unsafe { ffi::advertise(T::metadata(), initial) };
            // CAS in case of concurrent first-advertise. If someone
            // beat us, drop our handle (it'd reference the same broker
            // entry on host; on target PX4 dedupes by topic name).
            if self
                .handle
                .compare_exchange(
                    core::ptr::null_mut(),
                    h,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                // SAFETY: we just allocated h via advertise.
                unsafe { ffi::unadvertise(h) };
            }
        }
    }

    /// Publish one sample.
    pub fn publish(&self, msg: &T::Msg) -> Result<(), PubError> {
        self.ensure_advertised(msg as *const _ as *const c_void);
        let handle = self.handle.load(Ordering::Acquire);
        // SAFETY: handle is non-null after ensure_advertised; msg is a
        // valid T::Msg of the right size.
        let rc = unsafe { ffi::publish(T::metadata(), handle, msg as *const _ as *const c_void) };
        if rc == 0 {
            Ok(())
        } else {
            Err(PubError::Failed)
        }
    }

    /// Convenience: zero-init and publish in one call.
    pub fn publish_zeroed(&self) -> Result<(), PubError>
    where
        T::Msg: Copy,
    {
        let zero: MaybeUninit<T::Msg> = MaybeUninit::zeroed();
        // SAFETY: T::Msg is #[repr(C)] POD; zero is a valid bit pattern.
        let z = unsafe { zero.assume_init() };
        self.publish(&z)
    }
}

impl<T: UorbTopic> Default for Publication<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: UorbTopic> Drop for Publication<T> {
    fn drop(&mut self) {
        let h = self.handle.swap(core::ptr::null_mut(), Ordering::AcqRel);
        if !h.is_null() {
            // SAFETY: we own h.
            unsafe { ffi::unadvertise(h) };
        }
    }
}
