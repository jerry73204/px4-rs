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
    //! Host mock: a single worker thread services every pending
    //! timer via a sorted-by-deadline priority queue. `cancel` flips
    //! a flag in the entry so the worker skips invoking the callout
    //! if the awaiting Sleep was dropped before the deadline.
    //!
    //! Earlier revisions spawned a fresh thread per `call_after`. That
    //! design exhausted process thread limits under parallel
    //! integration tests and surfaced as
    //! `tcache_thread_shutdown: unaligned tcache chunk detected`
    //! at process exit because dozens of unjoined helper threads
    //! were still mid-cleanup when `main` returned. The single-worker
    //! model bounds thread count at one and avoids the race.
    //!
    //! Semantics aren't bit-identical to PX4's HRT (no jitter
    //! compensation, no priority bands) but match what unit tests
    //! need: ordered timer fires that wake the right task.

    use core::ffi::c_void;
    use std::collections::BinaryHeap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    /// Mirrors `px4_sys::hrt_call`: 64-byte opaque buffer. We only
    /// use the first byte (a `cancelled` flag) but keep the size so
    /// the timer future's storage layout matches the target build.
    #[repr(C)]
    pub(crate) struct HrtCall {
        cancelled: AtomicBool,
        _pad: [u8; 63],
    }

    pub(crate) type Callout = unsafe extern "C" fn(*mut c_void);

    /// One pending timer. `entry_addr` and `ctx_addr` are
    /// type-erased to `usize` so the heap entries are `Send` —
    /// raw pointers are not Send by default.
    struct Pending {
        deadline: Instant,
        entry_addr: usize,
        ctx_addr: usize,
        callout: Callout,
    }

    impl PartialEq for Pending {
        fn eq(&self, other: &Self) -> bool {
            self.deadline == other.deadline
        }
    }
    impl Eq for Pending {}
    impl PartialOrd for Pending {
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Pending {
        fn cmp(&self, other: &Self) -> core::cmp::Ordering {
            // BinaryHeap is a max-heap; invert so earliest deadline
            // comes out first.
            other.deadline.cmp(&self.deadline)
        }
    }

    struct Worker {
        queue: Mutex<BinaryHeap<Pending>>,
        cv: Condvar,
    }

    fn worker() -> &'static Worker {
        static W: OnceLock<Worker> = OnceLock::new();
        W.get_or_init(|| {
            let w = Worker {
                queue: Mutex::new(BinaryHeap::new()),
                cv: Condvar::new(),
            };
            // The static outlives the spawned thread; reading W after
            // get_or_init is always Some.
            thread::Builder::new()
                .name("px4_rs_mock_hrt".into())
                .spawn(run_worker)
                .expect("spawn mock hrt worker");
            w
        })
    }

    fn run_worker() {
        let w = worker();
        loop {
            let mut q = w.queue.lock().unwrap();
            // Wait for either work to arrive or the next deadline.
            loop {
                match q.peek() {
                    Some(top) => {
                        let now = Instant::now();
                        if top.deadline <= now {
                            break;
                        }
                        let timeout = top.deadline - now;
                        let (g, _) = w.cv.wait_timeout(q, timeout).unwrap();
                        q = g;
                    }
                    None => {
                        q = w.cv.wait(q).unwrap();
                    }
                }
            }
            // pop one expired entry per loop pass — keeps per-call
            // ordering deterministic.
            let p = q.pop().unwrap();
            drop(q);
            // SAFETY: `entry_addr` was a `*mut HrtCall` from the
            // caller's pinned Sleep storage. `cancel()` only flips
            // the bool — never frees — so reading `cancelled`
            // before the Sleep is dropped is sound. The Sleep
            // promises (via `_pin: PhantomPinned` + `Drop`) that the
            // storage outlives any pending callback.
            let cancelled = unsafe {
                (*(p.entry_addr as *const HrtCall))
                    .cancelled
                    .load(Ordering::Acquire)
            };
            if cancelled {
                continue;
            }
            // SAFETY: same guarantee — Sleep outlives the callout.
            unsafe { (p.callout)(p.ctx_addr as *mut c_void) };
        }
    }

    pub(crate) unsafe fn call_after(
        entry: *mut HrtCall,
        delay_us: u64,
        callout: Callout,
        ctx: *mut c_void,
    ) {
        // SAFETY: caller guarantees `entry` points to a buffer big
        // enough for HrtCall. Initialise the cancelled flag.
        unsafe {
            (*entry).cancelled.store(false, Ordering::Release);
        }
        let w = worker();
        let deadline = Instant::now() + Duration::from_micros(delay_us);
        let pending = Pending {
            deadline,
            entry_addr: entry as usize,
            ctx_addr: ctx as usize,
            callout,
        };
        let mut q = w.queue.lock().unwrap();
        q.push(pending);
        w.cv.notify_one();
    }

    pub(crate) unsafe fn cancel(entry: *mut HrtCall) {
        // Flip the in-band flag first — if the worker is mid-`peek`
        // it will read this and skip the dispatch.
        // SAFETY: caller guarantees `entry` is a valid HrtCall
        // initialised by a prior `call_after`.
        unsafe {
            (*entry).cancelled.store(true, Ordering::Release);
        }
        // Then evict the entry from the priority queue.
        //
        // Without this, the worker keeps the stale `entry_addr` —
        // a pointer into the awaiting `Sleep`'s storage — until the
        // deadline. If `Sleep` drops first (cancellation case), the
        // queue holds a dangling pointer; reading `cancelled` from
        // it later races free + reallocate of the same address and
        // surfaces as glibc's `tcache_thread_shutdown: unaligned
        // tcache chunk detected` at process exit.
        //
        // O(n) over the queue, but cancellations are rare and the
        // queue is short (one entry per outstanding Sleep).
        let entry_addr = entry as usize;
        let w = worker();
        let mut q = w.queue.lock().unwrap();
        // BinaryHeap doesn't support remove-by-value. Drain into a
        // Vec, filter, push back. Order is rebuilt by BinaryHeap's
        // From<Vec> heapify.
        let kept: Vec<Pending> = q.drain().filter(|p| p.entry_addr != entry_addr).collect();
        *q = BinaryHeap::from(kept);
        // Wake the worker so it re-evaluates its next deadline.
        w.cv.notify_one();
    }
}
