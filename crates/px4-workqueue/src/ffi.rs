//! Indirection layer for the `px4_rs_wi_*` FFI. On `std` host builds
//! the calls are intercepted by an in-process mock runtime so tests
//! don't need a PX4 link target.

#![cfg_attr(feature = "std", allow(dead_code))]

pub(crate) use px4_sys::px4_rs_wq_config as WqConfigFfi;

#[cfg(not(feature = "std"))]
pub(crate) use real::*;

#[cfg(feature = "std")]
pub(crate) use mock::*;

#[cfg(not(feature = "std"))]
mod real {
    use core::ffi::{c_char, c_void};

    #[doc(hidden)]
    pub enum WorkItem {}

    pub(crate) type RunFn = unsafe extern "C" fn(*mut c_void);

    pub(crate) unsafe fn wi_new(
        cfg: *const super::WqConfigFfi,
        name: *const c_char,
        ctx: *mut c_void,
        run: Option<RunFn>,
    ) -> *mut WorkItem {
        unsafe { px4_sys::px4_rs_wi_new(cfg, name, ctx, run).cast() }
    }

    pub(crate) unsafe fn wi_schedule_now(wi: *mut WorkItem) {
        unsafe { px4_sys::px4_rs_wi_schedule_now(wi.cast()) }
    }
}

#[cfg(feature = "std")]
pub(crate) mod mock {
    //! In-process WorkQueue mock. A single dispatcher thread consumes
    //! ScheduleNow requests and calls the registered `run` callback —
    //! a direct analogue of PX4's `WorkQueue::Run()`.

    use core::ffi::{c_char, c_void};
    use std::boxed::Box;
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Mutex, OnceLock};
    use std::thread;

    #[doc(hidden)]
    pub struct WorkItem {
        ctx: usize,
        run: super::RunFn,
    }

    pub(crate) type RunFn = unsafe extern "C" fn(*mut c_void);

    type Sched = Sender<usize>; // work-item address

    fn scheduler() -> &'static Mutex<Option<Sched>> {
        static S: OnceLock<Mutex<Option<Sched>>> = OnceLock::new();
        S.get_or_init(|| Mutex::new(None))
    }

    fn ensure_dispatcher() -> Sched {
        let mut guard = scheduler().lock().unwrap();
        if let Some(s) = guard.as_ref() {
            return s.clone();
        }
        let (tx, rx) = channel::<usize>();
        thread::Builder::new()
            .name("px4_rs_mock_wq".into())
            .spawn(move || {
                while let Ok(addr) = rx.recv() {
                    // SAFETY: the address comes from a `Box::leak`-ed
                    // WorkItem whose lifetime is 'static. We never free.
                    let wi: &'static WorkItem = unsafe { &*(addr as *const WorkItem) };
                    unsafe { (wi.run)(wi.ctx as *mut c_void) };
                }
            })
            .expect("spawn mock dispatcher");
        *guard = Some(tx.clone());
        tx
    }

    pub(crate) unsafe fn wi_new(
        _cfg: *const super::WqConfigFfi,
        _name: *const c_char,
        ctx: *mut c_void,
        run: Option<RunFn>,
    ) -> *mut WorkItem {
        let run = run.expect("run callback required");
        // Force dispatcher up so the first schedule has somewhere to go.
        let _ = ensure_dispatcher();
        let wi = Box::new(WorkItem {
            ctx: ctx as usize,
            run,
        });
        Box::leak(wi) as *mut WorkItem
    }

    pub(crate) unsafe fn wi_schedule_now(wi: *mut WorkItem) {
        let tx = ensure_dispatcher();
        let _ = tx.send(wi as usize);
    }

    /// Block the current thread until the dispatcher's queue drains.
    /// Test-only helper — we approximate "idle" by a short sleep loop,
    /// which is adequate for the small deterministic tests we run.
    #[doc(hidden)]
    pub fn drain_until_idle() {
        use std::time::Duration;
        // Give the dispatcher a beat to run any in-flight sends. Good
        // enough for single-producer tests; no guarantees for heavy
        // concurrent workloads.
        thread::sleep(Duration::from_millis(10));
    }
}
