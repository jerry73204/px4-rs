//! `multi_task` — two `#[task]`s on different WorkQueues talking via `Notify`.
//!
//! Idiomatic PX4 split: one task does the time-driven nudging and
//! another does the heavier work, each on its own WQ thread so they
//! can preempt independently. The producer runs on `hp_default` and
//! pings a `Notify` once a second; the consumer runs on `lp_default`,
//! awaits the notification, and bumps a counter.
//!
//! Run with:
//!
//! ```text
//! pxh> multi_task start
//! INFO  [multi_task] producer started
//! INFO  [multi_task] consumer started
//! INFO  [multi_task] consumer woke, count=1
//! INFO  [multi_task] consumer woke, count=2
//! …
//! ```

#![no_std]
#![feature(type_alias_impl_trait)]

use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

use px4::{Args, Notify, info, main, panic_handler, sleep, task};

panic_handler!();

static SIGNAL: Notify = Notify::new();
static WAKES: AtomicU32 = AtomicU32::new(0);

#[task(wq = "hp_default")]
async fn producer() {
    info!("producer started");
    loop {
        sleep(Duration::from_secs(1)).await;
        SIGNAL.notify();
    }
}

#[task(wq = "lp_default")]
async fn consumer() {
    info!("consumer started");
    loop {
        SIGNAL.notified().await;
        let n = WAKES.fetch_add(1, Ordering::AcqRel) + 1;
        info!("consumer woke, count={n}");
    }
}

#[main]
fn main(args: Args) -> Result<(), &'static str> {
    match args.subcommand() {
        Some(b"start") => {
            // Spawn the consumer first so a producer notify that
            // races with the consumer's first poll always lands as a
            // stored permit, never on a not-yet-registered waiter.
            consumer::try_spawn()
                .map_err(|_| "consumer already running")?
                .forget();
            producer::try_spawn()
                .map_err(|_| "producer already running")?
                .forget();
            info!("started");
            Ok(())
        }
        Some(b"status") => {
            let n = WAKES.load(Ordering::Acquire);
            info!("running, wake count={n}");
            Ok(())
        }
        Some(b"stop") => {
            info!("stop is a no-op in this example");
            Ok(())
        }
        _ => Err("usage: multi_task {start|stop|status}"),
    }
}
