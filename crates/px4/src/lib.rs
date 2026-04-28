//! `px4` — Rusty facade for PX4 Autopilot.
//!
//! One crate to depend on; one prefix to import. The actual
//! implementations live in the dedicated workspace crates
//! (`px4-log`, `px4-workqueue`, `px4-uorb`, `px4-sys`); `px4` is
//! glue that re-exports the user-facing surface under a single
//! namespace.
//!
//! # Hello, PX4
//!
//! ```ignore
//! #![no_std]
//! #![feature(type_alias_impl_trait)]
//!
//! use px4::{main, info, panic_handler, task, Args};
//!
//! panic_handler!();
//!
//! #[task(wq = "lp_default")]
//! async fn ticker() {
//!     info!("alive");
//! }
//!
//! #[main]                          // name = CARGO_PKG_NAME
//! fn main(args: Args) -> Result<(), &'static str> {
//!     match args.subcommand() {
//!         Some(b"start") => {
//!             ticker::try_spawn().map_err(|_| "already running")?;
//!             Ok(())
//!         }
//!         Some(b"status") => Ok(()),
//!         Some(b"stop")   => Ok(()),
//!         _ => Err("usage: hello {start|stop|status}"),
//!     }
//! }
//! ```
//!
//! That's a complete PX4 module: 15 lines, one import, no raw
//! `argc/argv`, no `extern "C"`, no `unsafe`.
//!
//! # What lives where
//!
//! - **Module entry** — [`main`], [`Args`], [`ModuleResult`]: wrap
//!   the C `<name>_main(int, char**)` shape PX4's pxh shell expects.
//!   `#[main]` defaults the module name from `CARGO_PKG_NAME`;
//!   override with `#[main(name = "...")]`.
//! - **Logging** — [`info!`], [`warn!`], [`err!`], [`debug!`],
//!   [`module!`] (legacy — `#[main]` covers it), [`panic_handler!`],
//!   [`Level`].
//! - **Async runtime** — [`task`] (attribute), [`WorkItemCell`],
//!   [`SpawnError`], [`SpawnToken`], [`WqConfig`],
//!   [`wq_configurations`], [`Sleep`], [`sleep`], [`Notify`],
//!   [`Channel`], [`AtomicWaker`].
//! - **uORB pub/sub** — [`Publication`], [`Subscription`],
//!   [`PubError`], [`OrbMetadata`], [`UorbTopic`], [`px4_message`]
//!   (attribute).
//! - **Raw FFI** — [`sys`] re-exports the entire `px4_sys` crate
//!   for advanced callers reaching past the typed wrappers.
//!
//! Three sub-types collide on simple names — `Recv` and `Send` exist
//! on both `Channel` and `Subscription`, and `Send` clashes with
//! `core::marker::Send`. They stay namespaced under [`workqueue`]
//! and [`uorb`] rather than being flattened.

#![cfg_attr(not(feature = "std"), no_std)]

// ---------------------------------------------------------------- entry
pub use px4_log::{Args, ArgsIter, ModuleResult};
pub use px4_macros::main;

// ---------------------------------------------------------------- log
pub use px4_log::{Level, debug, err, info, module, panic_handler, warn};

// ---------------------------------------------------------------- runtime
pub use px4_workqueue::{
    AtomicWaker, Channel, Notify, Sleep, SpawnError, SpawnToken, WorkItemCell, WqConfig, sleep,
    wq_configurations, yield_now,
};
pub use px4_workqueue_macros::task;

// ---------------------------------------------------------------- uORB
pub use px4_msg_macros::px4_message;
pub use px4_uorb::{OrbMetadata, PubError, Publication, Subscription, UorbTopic};

// ---------------------------------------------------------------- raw FFI
pub use px4_sys as sys;

/// Lower-level future types from the async runtime.
///
/// `Recv`/`Send`/`Notified` would shadow common names if pulled into
/// the crate root — `Send` collides with `core::marker::Send`, and
/// `Recv` collides with [`uorb::Recv`].
pub mod workqueue {
    pub use px4_workqueue::{Notified, Recv, Send};
}

/// Lower-level future types from the uORB layer.
pub mod uorb {
    pub use px4_uorb::Recv;
}
