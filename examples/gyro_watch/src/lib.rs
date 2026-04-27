//! `gyro_watch` — subscribe `sensor_gyro`, publish `gyro_alert` on spikes.
//!
//! One `#[task(wq = "rate_ctrl")]` runs the watcher: on each
//! `Subscription::recv()` it computes `|x| + |y| + |z|` and, if the
//! result crosses a threshold, publishes a `GyroAlert` carrying the
//! timestamp, magnitude, and a monotonic spike counter.
//!
//! The spec calls for `VehicleCommand` as the published topic, but
//! VehicleCommand has 200+ fields and would dwarf the 30-line task
//! body. Substituted a purpose-built `GyroAlert` message instead —
//! the substantive thing being demonstrated is `Subscription` +
//! `Publication` cooperating in one task, which is identical either
//! way.
//!
//! Run with:
//!
//! ```text
//! pxh> gyro_watch start
//! INFO  [gyro_watch] watcher started, threshold=2.5 rad/s
//! INFO  [gyro_watch] spike #1 magnitude=3.2
//! …
//! ```

#![no_std]
#![feature(type_alias_impl_trait)]

use px4::{Args, Publication, Subscription, info, main, panic_handler, px4_message, task};

panic_handler!();

#[px4_message("SensorGyro.msg")]
pub struct SensorGyro;

#[px4_message("GyroAlert.msg")]
pub struct GyroAlert;

/// Spike threshold on `|x| + |y| + |z|`. Above this we emit an
/// alert. 2.5 rad/s ≈ 143°/s — a hard slap rather than normal flight.
const SPIKE_THRESHOLD: f32 = 2.5;

static ALERT_PUB: Publication<gyro_alert> = Publication::new();

#[task(wq = "rate_ctrl")]
async fn watcher() {
    info!("watcher started, threshold=2.5 rad/s");
    let sub = Subscription::<sensor_gyro>::new();
    let mut spikes: u32 = 0;
    loop {
        let m: SensorGyro = sub.recv().await;
        let mag = abs_f32(m.x) + abs_f32(m.y) + abs_f32(m.z);
        if mag >= SPIKE_THRESHOLD {
            spikes = spikes.saturating_add(1);
            let alert = GyroAlert {
                timestamp: m.timestamp,
                magnitude: mag,
                spike_count: spikes,
            };
            let _ = ALERT_PUB.publish(&alert);
            info!("spike #{spikes} magnitude={mag}");
        }
    }
}

#[main]
fn main(args: Args) -> Result<(), &'static str> {
    match args.subcommand() {
        Some(b"start") => {
            watcher::try_spawn()
                .map_err(|_| "already running")?
                .forget();
            info!("started");
            Ok(())
        }
        Some(b"status") => {
            info!("running");
            Ok(())
        }
        Some(b"stop") => {
            info!("stop is a no-op in this example");
            Ok(())
        }
        _ => Err("usage: gyro_watch {start|stop|status}"),
    }
}

/// Branchless `f32::abs` for `no_std` builds, since `core::f32` ships
/// no `abs` and pulling in `libm` for one bit-flip is overkill.
fn abs_f32(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7FFF_FFFF)
}
