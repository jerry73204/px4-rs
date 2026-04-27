//! `px4-workqueue` — Rust async runtime on top of PX4's WorkQueue.
//!
//! Each task is one PX4 `WorkItem`. The Rust waker vtable calls
//! PX4's `ScheduleNow` directly; there is no second ready-queue inside
//! the runtime. See `docs/async-model.md` for the design rationale.
//!
//! # Example (manual, without the `#[task]` macro)
//!
//! ```ignore
//! use core::future::Future;
//! use px4_workqueue::{WorkItemCell, wq_configurations};
//!
//! async fn rate_watch() { /* loop body */ }
//!
//! static CELL: WorkItemCell<impl Future<Output = ()>> = WorkItemCell::new();
//!
//! pub fn start() {
//!     CELL.spawn(rate_watch(), &wq_configurations::rate_ctrl, c"rate_watch")
//!         .forget();
//! }
//! ```
//!
//! In normal use, `#[px4_workqueue_macros::task(wq = "rate_ctrl")]`
//! generates the static and the `spawn` function for you.

#![cfg_attr(not(feature = "std"), no_std)]

mod atomic_waker;
mod cell;
mod channel;
mod ffi;
mod hrt;
mod notify;
mod timer;
mod waker;
mod wq;

pub use atomic_waker::AtomicWaker;
pub use cell::{SpawnError, SpawnToken, WorkItemCell};
pub use channel::{Channel, Recv, Send};
pub use notify::{Notified, Notify};
pub use timer::{Sleep, sleep};
pub use wq::{WqConfig, wq_configurations};

pub use px4_workqueue_macros::task;

#[cfg(feature = "std")]
pub use ffi::mock::drain_until_idle;
