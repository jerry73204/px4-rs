//! Indirection layer for the uORB FFI. Real px4-sys calls on target;
//! an in-process broker on `std` host builds.

#![cfg_attr(feature = "std", allow(dead_code))]

#[cfg(not(feature = "std"))]
pub(crate) use real::*;

#[cfg(feature = "std")]
pub(crate) use mock::*;

#[cfg(not(feature = "std"))]
mod real {
    use core::ffi::c_void;

    use px4_sys::{orb_metadata, px4_rs_sub_cb};

    pub(crate) type SubCb = px4_rs_sub_cb;
    pub(crate) type OrbAdvert = *mut c_void;

    pub(crate) unsafe fn advertise(
        meta: &'static orb_metadata,
        initial: *const c_void,
    ) -> OrbAdvert {
        let mut instance = 0i32;
        unsafe { px4_sys::orb_advertise_multi(meta, initial, &mut instance) }
    }

    /// Advertise on a specific instance. PX4 may reassign; the actual
    /// instance is written back through `instance_inout`.
    pub(crate) unsafe fn advertise_multi(
        meta: &'static orb_metadata,
        initial: *const c_void,
        instance_inout: &mut i32,
    ) -> OrbAdvert {
        unsafe { px4_sys::orb_advertise_multi(meta, initial, instance_inout) }
    }

    pub(crate) unsafe fn publish(
        meta: &'static orb_metadata,
        handle: OrbAdvert,
        data: *const c_void,
    ) -> i32 {
        unsafe { px4_sys::orb_publish(meta, handle, data) }
    }

    pub(crate) unsafe fn unadvertise(handle: OrbAdvert) {
        unsafe {
            let _ = px4_sys::orb_unadvertise(handle);
        }
    }

    pub(crate) unsafe fn sub_cb_new(
        meta: &'static orb_metadata,
        interval_us: u32,
        instance: u8,
        ctx: *mut c_void,
        call: unsafe extern "C" fn(*mut c_void),
    ) -> *mut SubCb {
        unsafe { px4_sys::px4_rs_sub_cb_new(meta, interval_us, instance, ctx, Some(call)) }
    }

    pub(crate) unsafe fn sub_cb_register(cb: *mut SubCb) -> bool {
        unsafe { px4_sys::px4_rs_sub_cb_register(cb) }
    }

    pub(crate) unsafe fn sub_cb_update(cb: *mut SubCb, dst: *mut c_void) -> bool {
        unsafe { px4_sys::px4_rs_sub_cb_update(cb, dst) }
    }

    #[allow(dead_code)]
    pub(crate) unsafe fn sub_cb_unregister(cb: *mut SubCb) {
        unsafe { px4_sys::px4_rs_sub_cb_unregister(cb) }
    }

    pub(crate) unsafe fn sub_cb_delete(cb: *mut SubCb) {
        unsafe { px4_sys::px4_rs_sub_cb_delete(cb) }
    }
}

#[cfg(feature = "std")]
pub(crate) mod mock {
    //! Single-process broker keyed by topic name (`o_name`).
    //!
    //! Each topic holds a most-recent payload buffer plus a list of
    //! registered callbacks. Publishing copies the payload, bumps a
    //! generation counter, and fires each callback. Subscribers track
    //! the last generation they observed so `try_recv` can return the
    //! most recent message without re-delivering.

    use core::ffi::c_void;
    use std::collections::HashMap;
    use std::ffi::CStr;
    use std::sync::{Arc, Mutex, OnceLock};

    use px4_sys::orb_metadata;

    pub(crate) type SubCb = SubCbInner;
    pub(crate) type OrbAdvert = *mut c_void;

    /// One subscriber's view of a topic.
    pub(crate) struct SubCbInner {
        topic: Arc<TopicState>,
        last_seen: Mutex<u64>,
        ctx: usize,
        call: Option<unsafe extern "C" fn(*mut c_void)>,
        registered: Mutex<bool>,
        size: usize,
    }

    /// Shared state for one topic.
    ///
    /// `callbacks` holds owning `Arc` clones of every registered
    /// `SubCbInner`. Snapshotting in `notify()` clones the Vec, so
    /// each in-flight dispatch holds its own refcount on every
    /// callback — a concurrent `Subscription::drop` (which decrements
    /// the original Arc via `sub_cb_delete`) cannot race-free the
    /// inner while the snapshot iteration is touching it.
    struct TopicState {
        size: usize,
        seq: Mutex<u64>,
        data: Mutex<Vec<u8>>,
        callbacks: Mutex<Vec<Arc<SubCbInner>>>,
    }

    fn broker() -> &'static Mutex<HashMap<String, Arc<TopicState>>> {
        static B: OnceLock<Mutex<HashMap<String, Arc<TopicState>>>> = OnceLock::new();
        B.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn topic_name(meta: &orb_metadata) -> String {
        unsafe { CStr::from_ptr(meta.o_name) }
            .to_string_lossy()
            .into_owned()
    }

    fn topic_for(meta: &'static orb_metadata) -> Arc<TopicState> {
        let name = topic_name(meta);
        let size = meta.o_size as usize;
        let mut g = broker().lock().unwrap();
        g.entry(name)
            .or_insert_with(|| {
                Arc::new(TopicState {
                    size,
                    seq: Mutex::new(0),
                    data: Mutex::new(vec![0u8; size]),
                    callbacks: Mutex::new(Vec::new()),
                })
            })
            .clone()
    }

    pub(crate) unsafe fn advertise(
        meta: &'static orb_metadata,
        initial: *const c_void,
    ) -> OrbAdvert {
        let topic = topic_for(meta);
        // Write the initial sample.
        unsafe {
            let mut data = topic.data.lock().unwrap();
            std::ptr::copy_nonoverlapping(initial as *const u8, data.as_mut_ptr(), topic.size);
        }
        // Bump the sequence so existing subscribers (whose last_seen
        // counters survive an unadvertise/re-advertise cycle) observe
        // this initial sample. Resetting to 1 unconditionally would
        // mask the new payload from any sub still parked at a higher
        // generation.
        *topic.seq.lock().unwrap() += 1;
        // Fire any callbacks that registered before the first publish.
        notify(&topic);
        // Leak an Arc clone as the handle.
        Arc::into_raw(topic) as *mut c_void
    }

    /// Mock has no notion of multi-instance — every publication of a
    /// given topic name shares one broker entry. We honour the request
    /// by writing back the same instance the caller passed in.
    pub(crate) unsafe fn advertise_multi(
        meta: &'static orb_metadata,
        initial: *const c_void,
        _instance_inout: &mut i32,
    ) -> OrbAdvert {
        unsafe { advertise(meta, initial) }
    }

    pub(crate) unsafe fn publish(
        _meta: &'static orb_metadata,
        handle: OrbAdvert,
        data_ptr: *const c_void,
    ) -> i32 {
        // SAFETY: handle came from advertise(), so it's an Arc<TopicState>.
        let topic = unsafe { Arc::from_raw(handle as *const TopicState) };
        let topic_clone = Arc::clone(&topic);
        // Don't drop our handle copy — the publisher still owns it.
        let _ = Arc::into_raw(topic);

        unsafe {
            let mut buf = topic_clone.data.lock().unwrap();
            std::ptr::copy_nonoverlapping(
                data_ptr as *const u8,
                buf.as_mut_ptr(),
                topic_clone.size,
            );
        }
        *topic_clone.seq.lock().unwrap() += 1;
        notify(&topic_clone);
        0
    }

    pub(crate) unsafe fn unadvertise(handle: OrbAdvert) {
        // Drop our Arc share. The topic survives if any subscribers
        // (or the broker map) still reference it.
        let _ = unsafe { Arc::from_raw(handle as *const TopicState) };
    }

    fn notify(topic: &Arc<TopicState>) {
        // Hold the callback list lock across dispatch.
        //
        // Two reasons we cannot snapshot-and-release:
        //
        // 1. `SubCbInner` lifetime — fixed by `Vec<Arc<SubCbInner>>`,
        //    each clone keeps the inner alive across iteration.
        // 2. **`ctx` lifetime** — the callback dereferences
        //    `cb.ctx`, which points into the awaiting `Subscription`
        //    (a `*const AtomicWaker`). If `Subscription::drop` runs
        //    between snapshot and dispatch, `sub_cb_delete` removes
        //    the entry from the list but the `ctx` pointer in our
        //    snapshot still aims at freed `Subscription` storage.
        //    The `Arc<SubCbInner>` clone protects the *inner* but
        //    not the *external* waker.
        //
        // Holding the lock across dispatch makes
        // `sub_cb_unregister` (called from `sub_cb_delete`) block
        // until the dispatch finishes, so the `Subscription` storage
        // stays alive for the duration of the call.
        //
        // The wake callbacks are short and side-effect-only
        // (`AtomicWaker::wake` → `Waker::wake_by_ref`). Holding the
        // lock for that long is fine. Pathological wake handlers
        // that re-enter the broker (e.g. publish-on-wake) would
        // deadlock — document that as unsupported.
        let cbs = topic.callbacks.lock().unwrap();
        for cb in cbs.iter() {
            if let Some(call) = cb.call {
                // SAFETY: `cb.ctx` was set by the awaiting
                // Subscription via `sub_cb_new`. The Subscription
                // cannot have run `Drop` while we hold this lock —
                // any concurrent `sub_cb_delete` blocks on the
                // `sub_cb_unregister` call inside it, which needs
                // the same lock.
                unsafe { call(cb.ctx as *mut c_void) };
            }
        }
    }

    /// Tracks every live `SubCbInner` (by inner-data address) for
    /// leak-debugging. Each address is the `Arc::into_raw` result
    /// returned to the FFI caller as a `*mut SubCb` handle.
    fn live_cbs() -> &'static Mutex<Vec<usize>> {
        static V: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
        V.get_or_init(|| Mutex::new(Vec::new()))
    }

    pub(crate) unsafe fn sub_cb_new(
        meta: &'static orb_metadata,
        _interval_us: u32,
        _instance: u8,
        ctx: *mut c_void,
        call: unsafe extern "C" fn(*mut c_void),
    ) -> *mut SubCb {
        let topic = topic_for(meta);
        let cb = Arc::new(SubCbInner {
            size: topic.size,
            topic,
            // Initial last_seen = 0; if a publish has already happened
            // (seq >= 1), the first try_recv returns the latest sample.
            last_seen: Mutex::new(0),
            ctx: ctx as usize,
            call: Some(call),
            registered: Mutex::new(false),
        });
        let raw = Arc::into_raw(cb) as *mut SubCb;
        live_cbs().lock().unwrap().push(raw as usize);
        raw
    }

    /// Borrow the SubCbInner pointed to by an FFI handle without
    /// taking ownership. The handle was returned by
    /// `Arc::into_raw(...)` and is reclaimed only by `sub_cb_delete`.
    ///
    /// SAFETY: caller must guarantee `cb` is a valid handle — it
    /// came from `sub_cb_new` and `sub_cb_delete` has not been
    /// called.
    unsafe fn cb_ref<'a>(cb: *mut SubCb) -> &'a SubCbInner {
        unsafe { &*(cb as *const SubCbInner) }
    }

    pub(crate) unsafe fn sub_cb_register(cb: *mut SubCb) -> bool {
        let inner = unsafe { cb_ref(cb) };
        let mut reg = inner.registered.lock().unwrap();
        if !*reg {
            // Push an extra Arc clone into the topic's callback
            // list. The clone keeps the inner alive across `notify`
            // dispatch even if the original Arc is dropped via
            // `sub_cb_delete` while a callback is in flight.
            // SAFETY: the original Arc lives at `cb` until
            // sub_cb_delete consumes it; we increment its refcount
            // by reconstructing-and-clone-and-leaking.
            let original = unsafe { Arc::from_raw(cb as *const SubCbInner) };
            inner
                .topic
                .callbacks
                .lock()
                .unwrap()
                .push(Arc::clone(&original));
            // Re-leak the original so the FFI handle stays valid.
            let _leak = Arc::into_raw(original);
            *reg = true;
        }
        true
    }

    pub(crate) unsafe fn sub_cb_update(cb: *mut SubCb, dst: *mut c_void) -> bool {
        let inner = unsafe { cb_ref(cb) };
        let cur = *inner.topic.seq.lock().unwrap();
        let mut last = inner.last_seen.lock().unwrap();
        if cur > *last {
            let data = inner.topic.data.lock().unwrap();
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, inner.size);
            }
            *last = cur;
            true
        } else {
            false
        }
    }

    pub(crate) unsafe fn sub_cb_unregister(cb: *mut SubCb) {
        let inner = unsafe { cb_ref(cb) };
        let mut reg = inner.registered.lock().unwrap();
        if *reg {
            // Remove the matching Arc clone from the callback list
            // by inner-data pointer identity. `Arc::as_ptr` returns
            // the same address that `Arc::into_raw` did.
            let cb_ptr = cb as *const SubCbInner;
            inner
                .topic
                .callbacks
                .lock()
                .unwrap()
                .retain(|arc| Arc::as_ptr(arc) != cb_ptr);
            *reg = false;
        }
    }

    pub(crate) unsafe fn sub_cb_delete(cb: *mut SubCb) {
        unsafe {
            sub_cb_unregister(cb);
            live_cbs().lock().unwrap().retain(|p| *p != cb as usize);
            // Reclaim the FFI handle's Arc and drop. If `notify` is
            // mid-dispatch on a snapshot containing this entry, that
            // snapshot's Arc clone keeps the inner alive — only the
            // last drop frees.
            drop(Arc::from_raw(cb as *const SubCbInner));
        }
    }

    /// Test-only — clear the entire broker so each test runs fresh.
    ///
    /// Releases broker-side `Arc<TopicState>` clones. `SubCbInner`
    /// instances still held by the calling test's Subscriptions
    /// stay alive (they hold their own `Arc<TopicState>`); fully
    /// dropping them is `sub_cb_delete`'s job, fired from
    /// `Subscription::drop`.
    pub fn _reset_broker() {
        broker().lock().unwrap().clear();
    }
}
