//! Single-slot, lock-free `Waker` store.
//!
//! Port of `futures_util::task::AtomicWaker`, simplified for `no_std`
//! and stripped of `unsafe` not needed for our use. One waker can be
//! registered at a time; re-register overwrites; `wake()` drains and
//! invokes.
//!
//! State machine (matches the upstream crate):
//!
//! ```text
//!   WAITING                         (idle)
//!     │
//!     ▼
//!   REGISTERING                     (a task is writing into the slot)
//!     │
//!     ▼
//!   WAITING                         (slot holds a Waker)
//! ```
//!
//! A concurrent `wake` transitions REGISTERING → REGISTERING|WAKING to
//! steal the registration, and WAITING → WAKING to invoke the stored
//! waker. The inline comments preserve the upstream ordering constants.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::Waker;

const WAITING: usize = 0;
const REGISTERING: usize = 0b01;
const WAKING: usize = 0b10;

pub struct AtomicWaker {
    state: AtomicUsize,
    waker: UnsafeCell<Option<Waker>>,
}

// SAFETY: synchronization is handled through `state`; direct access to
// `waker` is gated on having either REGISTERING or WAKING set.
unsafe impl Send for AtomicWaker {}
unsafe impl Sync for AtomicWaker {}

impl Default for AtomicWaker {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicWaker {
    pub const fn new() -> Self {
        Self {
            state: AtomicUsize::new(WAITING),
            waker: UnsafeCell::new(None),
        }
    }

    /// Register the current task's waker. If a previous waker was
    /// registered, it is replaced.
    pub fn register(&self, waker: &Waker) {
        match self
            .state
            .compare_exchange(WAITING, REGISTERING, Ordering::Acquire, Ordering::Acquire)
            .unwrap_or_else(|x| x)
        {
            WAITING => {
                // SAFETY: We hold the REGISTERING exclusive access.
                unsafe {
                    (*self.waker.get()) = Some(waker.clone());
                }

                match self.state.compare_exchange(
                    REGISTERING,
                    WAITING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {}
                    Err(actual) => {
                        // A concurrent `wake` set WAKING. Drain the
                        // slot ourselves and invoke.
                        debug_assert_eq!(actual, REGISTERING | WAKING);
                        // SAFETY: same REGISTERING access.
                        let waker = unsafe { (*self.waker.get()).take() };
                        self.state.swap(WAITING, Ordering::AcqRel);
                        if let Some(w) = waker {
                            w.wake();
                        }
                    }
                }
            }
            WAKING => {
                // A wake is in flight; just trigger this one immediately
                // so the task isn't lost.
                waker.wake_by_ref();
            }
            state => {
                // Two registrations raced, or a wake is draining.
                // Nothing to do: the other thread owns the slot.
                debug_assert!(state == REGISTERING || state == REGISTERING | WAKING);
            }
        }
    }

    /// Wake the most recently registered task, if any.
    pub fn wake(&self) {
        if let Some(w) = self.take() {
            w.wake();
        }
    }

    /// Remove and return the currently stored waker, if any.
    pub fn take(&self) -> Option<Waker> {
        match self.state.fetch_or(WAKING, Ordering::AcqRel) {
            WAITING => {
                // SAFETY: we just promoted to WAKING with exclusive
                // access over the slot.
                let waker = unsafe { (*self.waker.get()).take() };
                self.state.fetch_and(!WAKING, Ordering::Release);
                waker
            }
            _ => {
                // Someone else owns the slot (registering or waking).
                // They'll see our WAKING bit and honour it.
                None
            }
        }
    }
}
