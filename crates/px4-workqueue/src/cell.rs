//! Per-task static cell and spawn API.
//!
//! A `WorkItemCell<F>` is the static storage for one task. It owns:
//!   * a state word (`AtomicU8` with `SPAWNED` / `RUN_QUEUED` bits)
//!   * a pointer to the PX4 `WorkItem` we've created for this task
//!   * a `MaybeUninit<F>` slot for the future
//!
//! The `#[repr(C)]` prefix `TaskStateBits` is what `Waker`s point at,
//! so a single universal `RawWakerVTable` can service every F.

use core::cell::UnsafeCell;
use core::future::Future;
use core::marker::PhantomPinned;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
use core::task::{Context, Poll, RawWaker, Waker};

use crate::ffi;
use crate::waker::WAKER_VTABLE;
use crate::wq::WqConfig;

/// State bits. Kept in a `#[repr(C)]` struct so the waker data pointer
/// can refer to just the bits (no type erasure needed on the waker path).
#[doc(hidden)]
#[repr(C)]
pub struct TaskStateBits {
    pub(crate) state: AtomicU8,
    pub(crate) handle: AtomicPtr<ffi::WorkItem>,
}

/// `state` layout:
///   * `SPAWNED` — future is live in the cell, work item is bound to a
///     WorkQueue, and `handle` is non-null.
///   * `RUN_QUEUED` — a `ScheduleNow` has been requested for this item
///     and hasn't been consumed by a `Run()` yet.
pub(crate) const SPAWNED: u8 = 0b0000_0001;
pub(crate) const RUN_QUEUED: u8 = 0b0000_0010;

/// Static-allocated storage for one task's future and work-item handle.
///
/// Generic over the concrete future type `F`. Place in a `static`:
///
/// ```ignore
/// static CELL: WorkItemCell<MyFut> = WorkItemCell::new();
/// ```
#[repr(C)]
pub struct WorkItemCell<F> {
    pub(crate) bits: TaskStateBits,
    pub(crate) future: UnsafeCell<MaybeUninit<F>>,
    _pin: PhantomPinned,
}

// SAFETY: a WorkItemCell is owned end-to-end by the PX4 WorkQueue
// pthread it's attached to; we never poll the future from any other
// thread. The Sync impl is required so that `static CELL: WorkItemCell<F>`
// is permitted, but no actual cross-thread access of the inner future
// or state ever happens.
unsafe impl<F> Send for WorkItemCell<F> {}
unsafe impl<F> Sync for WorkItemCell<F> {}

impl<F: Future<Output = ()> + 'static> WorkItemCell<F> {
    pub const fn new() -> Self {
        Self {
            bits: TaskStateBits {
                state: AtomicU8::new(0),
                handle: AtomicPtr::new(ptr::null_mut()),
            },
            future: UnsafeCell::new(MaybeUninit::uninit()),
            _pin: PhantomPinned,
        }
    }

    /// Attempt to spawn this task. Returns `Err(SpawnError::Busy)` if
    /// the task is already running. Returns a `SpawnToken` that must
    /// be passed to an executor (currently a no-op: construction
    /// already scheduled the first poll — the token guards against
    /// forgotten spawn results).
    pub fn try_spawn(
        &'static self,
        fut: F,
        wq: &'static WqConfig,
        name: &'static core::ffi::CStr,
    ) -> Result<SpawnToken, SpawnError> {
        match self.bits.state.compare_exchange(
            0,
            SPAWNED | RUN_QUEUED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(_) => return Err(SpawnError::Busy),
        }

        // SAFETY: SPAWNED just flipped to set, so this is the only
        // writer for the `future` slot until the future resolves.
        unsafe {
            (*self.future.get()).as_mut_ptr().write(fut);
        }

        // First-ever spawn allocates the WorkItem. Subsequent respawns
        // reuse the same handle (PX4's ScheduledWorkItem is re-arm-able).
        let mut handle = self.bits.handle.load(Ordering::Acquire);
        if handle.is_null() {
            // SAFETY: passing our own static pointer as ctx. Lives forever.
            let ctx = &self.bits as *const TaskStateBits as *mut core::ffi::c_void;
            handle =
                unsafe { ffi::wi_new(wq.as_ffi(), name.as_ptr(), ctx, Some(Self::run_trampoline)) };
            if handle.is_null() {
                // PX4 couldn't allocate. Roll back state.
                unsafe {
                    (*self.future.get()).assume_init_drop();
                }
                self.bits.state.store(0, Ordering::Release);
                return Err(SpawnError::AllocFailed);
            }
            self.bits.handle.store(handle, Ordering::Release);
        }

        // First schedule. Subsequent wakes go through the RawWaker vtable.
        // SAFETY: handle is non-null and owned by PX4's WorkQueueManager
        // for the lifetime of this static.
        unsafe {
            ffi::wi_schedule_now(handle);
        }

        Ok(SpawnToken::new())
    }

    /// Same as `try_spawn` but panics on `Busy`. Use for cold-start code
    /// where a second spawn is a programmer error.
    pub fn spawn(
        &'static self,
        fut: F,
        wq: &'static WqConfig,
        name: &'static core::ffi::CStr,
    ) -> SpawnToken {
        match self.try_spawn(fut, wq, name) {
            Ok(t) => t,
            Err(e) => panic!("px4-workqueue: spawn failed: {e:?}"),
        }
    }

    /// Called by the C++ trampoline on every WorkQueue run.
    ///
    /// Matches the `void (*run)(void *ctx)` signature of
    /// `px4_sys::px4_rs_wi_new`. `ctx` is a `*const TaskStateBits`
    /// (the start of a `WorkItemCell<F>`).
    unsafe extern "C" fn run_trampoline(ctx: *mut core::ffi::c_void) {
        // SAFETY: ctx was passed from our own static's address.
        let cell: &'static WorkItemCell<F> = unsafe { &*(ctx as *const WorkItemCell<F>) };
        cell.poll_once();
    }

    fn poll_once(&'static self) {
        let prev = self.bits.state.fetch_and(!RUN_QUEUED, Ordering::AcqRel);
        if prev & SPAWNED == 0 {
            // The future already resolved; a stale ScheduleNow fired.
            return;
        }

        // Build a Waker pointing at our `bits`. The vtable is universal.
        // SAFETY: bits lives as long as the static; vtable is 'static.
        let raw = RawWaker::new(
            &self.bits as *const TaskStateBits as *const (),
            &WAKER_VTABLE,
        );
        let waker = unsafe { Waker::from_raw(raw) };
        let mut cx = Context::from_waker(&waker);

        // SAFETY: SPAWNED is set, so `future` contains an initialized F,
        // and this poll is serialized by the owning WorkQueue pthread.
        let pinned: Pin<&mut F> =
            unsafe { Pin::new_unchecked(&mut *(*self.future.get()).as_mut_ptr()) };

        match pinned.poll(&mut cx) {
            Poll::Pending => {}
            Poll::Ready(()) => {
                // SAFETY: SPAWNED is set — `future` is initialized.
                unsafe {
                    (*self.future.get()).assume_init_drop();
                }
                // Clear SPAWNED last so a racing spawn() can succeed.
                self.bits.state.store(0, Ordering::Release);
            }
        }
    }
}

impl<F: Future<Output = ()> + 'static> Default for WorkItemCell<F> {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-cost witness that a task was spawned successfully. The executor
/// is PX4's WorkQueue — which the spawn call has already scheduled — so
/// holding a token has no effect. Dropping one without using it (or
/// `.forget()`ing it) panics to flag a forgotten spawn result.
#[must_use = "a SpawnToken must be consumed; dropping without calling .forget() panics"]
#[derive(Debug)]
pub struct SpawnToken {
    _unused: core::marker::PhantomData<*const ()>,
}

impl SpawnToken {
    fn new() -> Self {
        Self {
            _unused: core::marker::PhantomData,
        }
    }

    /// Acknowledge the token and silence the drop-panic. Call this
    /// after the spawn is known to be live.
    pub fn forget(self) {
        core::mem::forget(self);
    }
}

impl Drop for SpawnToken {
    fn drop(&mut self) {
        panic!("SpawnToken dropped without being consumed — forgotten spawn result");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    /// The task is already running. Respawn will succeed after its
    /// future resolves.
    Busy,
    /// PX4's WorkQueueManager refused to allocate the WorkItem
    /// (typically out-of-memory or unknown WQ name).
    AllocFailed,
}

impl core::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Busy => f.write_str("task already running"),
            Self::AllocFailed => f.write_str("PX4 WorkQueueManager allocation failed"),
        }
    }
}
