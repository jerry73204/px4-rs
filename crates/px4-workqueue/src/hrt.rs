//! Indirection layer for `hrt_call_*`. Real px4-sys calls on target;
//! a std-thread-driven mock on host builds.

#![cfg_attr(feature = "std", allow(dead_code))]

#[cfg(not(feature = "std"))]
pub(crate) use real::*;

#[cfg(feature = "std")]
pub(crate) use mock::*;

#[cfg(not(feature = "std"))]
mod real {
    use core::ffi::c_void;

    pub(crate) type HrtCall = px4_sys::hrt_call;
    pub(crate) type Callout = unsafe extern "C" fn(*mut c_void);

    pub(crate) unsafe fn call_after(
        entry: *mut HrtCall,
        delay_us: u64,
        callout: Callout,
        ctx: *mut c_void,
    ) {
        unsafe { px4_sys::hrt_call_after(entry, delay_us, Some(callout), ctx) }
    }

    pub(crate) unsafe fn cancel(entry: *mut HrtCall) {
        unsafe { px4_sys::hrt_cancel(entry) }
    }
}

#[cfg(feature = "std")]
pub(crate) mod mock {
    //! Host mock: each `call_after` spawns a short-lived thread that
    //! sleeps and then invokes the callout. `cancel` flips a flag so
    //! the thread bails out quietly if it wakes up after the future
    //! has already been dropped.
    //!
    //! The semantics aren't bit-identical to PX4's HRT (no shared
    //! priority queue, no jitter compensation) but match what unit
    //! tests need: ordered timer fires that wake the right task.

    use core::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    /// Mirrors `px4_sys::hrt_call`: 64-byte opaque buffer. We only
    /// use the first byte (a `cancelled` flag) but keep the size so
    /// the timer future's storage layout matches the target build.
    #[repr(C)]
    pub(crate) struct HrtCall {
        cancelled: AtomicBool,
        _pad: [u8; 63],
    }

    pub(crate) type Callout = unsafe extern "C" fn(*mut c_void);

    pub(crate) unsafe fn call_after(
        entry: *mut HrtCall,
        delay_us: u64,
        callout: Callout,
        ctx: *mut c_void,
    ) {
        // SAFETY: caller guarantees `entry` points to a buffer big
        // enough for HrtCall. We initialise it in place.
        unsafe {
            (*entry).cancelled.store(false, Ordering::Release);
        }

        let entry_addr = entry as usize;
        let ctx_addr = ctx as usize;
        thread::Builder::new()
            .name("px4_rs_mock_hrt".into())
            .spawn(move || {
                thread::sleep(Duration::from_micros(delay_us));
                // SAFETY: `entry` is owned by the awaiting Sleep,
                // which is alive as long as `cancel()` hasn't been
                // called. Reading `cancelled` is the synchronisation
                // point; if it's false, the storage is still valid.
                let cancelled = unsafe {
                    (*(entry_addr as *const HrtCall))
                        .cancelled
                        .load(Ordering::Acquire)
                };
                if cancelled {
                    return;
                }
                // SAFETY: same guarantee — caller upholds that the
                // backing storage outlives the callout invocation.
                unsafe { callout(ctx_addr as *mut c_void) }
            })
            .expect("spawn mock hrt thread");
    }

    pub(crate) unsafe fn cancel(entry: *mut HrtCall) {
        // SAFETY: caller guarantees `entry` is a valid HrtCall
        // initialised by a prior `call_after`.
        unsafe {
            (*entry).cancelled.store(true, Ordering::Release);
        }
    }
}
