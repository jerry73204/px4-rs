//! RawWaker vtable. The data pointer is `&TaskStateBits` — the `#[repr(C)]`
//! prefix of every `WorkItemCell<F>` — so this vtable is universal
//! across all F.

use core::sync::atomic::Ordering;
use core::task::{RawWaker, RawWakerVTable};

use crate::cell::{RUN_QUEUED, SPAWNED, TaskStateBits};
use crate::ffi;

pub(crate) static WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

unsafe fn waker_clone(ptr: *const ()) -> RawWaker {
    // The cell lives in `'static` storage. Cloning is just copying
    // the pointer — no refcount.
    RawWaker::new(ptr, &WAKER_VTABLE)
}

unsafe fn waker_wake(ptr: *const ()) {
    unsafe { waker_wake_by_ref(ptr) };
}

unsafe fn waker_wake_by_ref(ptr: *const ()) {
    // SAFETY: ptr came from a WorkItemCell's &bits. That cell is a
    // static, so the pointer is always valid.
    let bits = unsafe { &*(ptr as *const TaskStateBits) };
    let prev = bits.state.fetch_or(RUN_QUEUED, Ordering::AcqRel);
    // Only schedule if the task is still live AND wasn't already queued.
    if (prev & SPAWNED) != 0 && (prev & RUN_QUEUED) == 0 {
        let handle = bits.handle.load(Ordering::Acquire);
        if !handle.is_null() {
            unsafe { ffi::wi_schedule_now(handle) };
        }
    }
}

unsafe fn waker_drop(_ptr: *const ()) {
    // No refcount, no owned resources. Nothing to do.
}
