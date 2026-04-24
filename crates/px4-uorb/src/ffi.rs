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
        ctx: *mut c_void,
        call: unsafe extern "C" fn(*mut c_void),
    ) -> *mut SubCb {
        unsafe { px4_sys::px4_rs_sub_cb_new(meta, 0, 0, ctx, Some(call)) }
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
    struct TopicState {
        size: usize,
        seq: Mutex<u64>,
        data: Mutex<Vec<u8>>,
        callbacks: Mutex<Vec<*const SubCbInner>>,
    }

    // SAFETY: callback pointers refer to leaked SubCbInner allocations
    // whose lifetime is global. The mutexes guard concurrent access.
    unsafe impl Send for TopicState {}
    unsafe impl Sync for TopicState {}

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
        *topic.seq.lock().unwrap() = 1;
        // Fire any callbacks that registered before the first publish.
        notify(&topic);
        // Leak an Arc clone as the handle.
        Arc::into_raw(topic) as *mut c_void
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
        let cbs = topic.callbacks.lock().unwrap().clone();
        for cb in cbs {
            // SAFETY: SubCbInner is leaked on creation; valid until sub_cb_delete.
            let cb_ref: &SubCbInner = unsafe { &*cb };
            if let Some(call) = cb_ref.call {
                unsafe { call(cb_ref.ctx as *mut c_void) };
            }
        }
    }

    pub(crate) unsafe fn sub_cb_new(
        meta: &'static orb_metadata,
        ctx: *mut c_void,
        call: unsafe extern "C" fn(*mut c_void),
    ) -> *mut SubCb {
        let topic = topic_for(meta);
        let cb = Box::new(SubCbInner {
            size: topic.size,
            topic,
            // Initial last_seen = 0; if a publish has already happened
            // (seq >= 1), the first try_recv returns the latest sample.
            last_seen: Mutex::new(0),
            ctx: ctx as usize,
            call: Some(call),
            registered: Mutex::new(false),
        });
        Box::into_raw(cb)
    }

    pub(crate) unsafe fn sub_cb_register(cb: *mut SubCb) -> bool {
        let cb_ref: &SubCbInner = unsafe { &*cb };
        let mut reg = cb_ref.registered.lock().unwrap();
        if !*reg {
            cb_ref.topic.callbacks.lock().unwrap().push(cb as *const _);
            *reg = true;
        }
        true
    }

    pub(crate) unsafe fn sub_cb_update(cb: *mut SubCb, dst: *mut c_void) -> bool {
        let cb_ref: &SubCbInner = unsafe { &*cb };
        let cur = *cb_ref.topic.seq.lock().unwrap();
        let mut last = cb_ref.last_seen.lock().unwrap();
        if cur > *last {
            let data = cb_ref.topic.data.lock().unwrap();
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, cb_ref.size);
            }
            *last = cur;
            true
        } else {
            false
        }
    }

    pub(crate) unsafe fn sub_cb_unregister(cb: *mut SubCb) {
        let cb_ref: &SubCbInner = unsafe { &*cb };
        let mut reg = cb_ref.registered.lock().unwrap();
        if *reg {
            cb_ref
                .topic
                .callbacks
                .lock()
                .unwrap()
                .retain(|p| *p != cb as *const _);
            *reg = false;
        }
    }

    pub(crate) unsafe fn sub_cb_delete(cb: *mut SubCb) {
        unsafe {
            sub_cb_unregister(cb);
            drop(Box::from_raw(cb));
        }
    }

    /// Test-only — clear the entire broker so each test runs fresh.
    pub fn _reset_broker() {
        broker().lock().unwrap().clear();
    }
}
