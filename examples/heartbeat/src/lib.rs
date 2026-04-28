//! `heartbeat` — minimal end-to-end PX4 Rust module.
//!
//! Publishes a synthetic `Airspeed` message in a tight loop with
//! `yield_now` between iterations so the WorkQueue can interleave.
//! Demonstrates `#[task]`, `#[px4_message]`, `Publication`, and the
//! CMake helper all at once.
//!
//! The publish rate is uncapped — for a real 1 Hz heartbeat use
//! `px4::sleep(Duration::from_secs(1))` instead. This example keeps
//! the busy-yield form because the build pipeline is what's being
//! exercised here, not the runtime primitive surface.

#![no_std]
#![feature(type_alias_impl_trait)]

use px4::{Args, Publication, info, main, panic_handler, px4_message, task, yield_now};

panic_handler!();

#[px4_message("Airspeed.msg")]
pub struct Airspeed;

static AIRSPEED_PUB: Publication<airspeed> = Publication::new();

#[task(wq = "lp_default")]
async fn pump() {
    info!("heartbeat task started");
    let mut counter: u64 = 0;
    loop {
        counter = counter.wrapping_add(1);
        let sample = Airspeed {
            timestamp: counter,
            timestamp_sample: counter,
            indicated_airspeed_m_s: 0.0,
            true_airspeed_m_s: 0.0,
            confidence: 1.0,
            _padding0: [0; 4],
        };
        if AIRSPEED_PUB.publish(&sample).is_err() {
            px4::err!("publish failed at counter {counter}");
        }
        yield_now().await;
    }
}

#[main]
fn main(args: Args) -> Result<(), &'static str> {
    match args.subcommand() {
        Some(b"start") => {
            pump::try_spawn().map_err(|_| "already running")?.forget();
            info!("started");
            Ok(())
        }
        Some(b"status") => {
            info!("running");
            Ok(())
        }
        Some(b"stop") => {
            // Phase 07 doesn't implement clean shutdown; document
            // the limitation rather than pretend.
            info!("stop is not implemented in this example");
            Ok(())
        }
        _ => Err("usage: heartbeat {start|stop|status}"),
    }
}
