//! `hello_module` — simplest possible px4-rs example.
//!
//! Spawns one `#[task]` on the `lp_default` work queue that prints
//! a `hello` line every second via `px4::sleep`. No uORB, no
//! multi-task plumbing — just the minimal scaffold of "task +
//! logger + timer".
//!
//! Run with:
//!
//! ```text
//! pxh> hello_module start
//! INFO  [hello_module] hello tick=1
//! INFO  [hello_module] hello tick=2
//! …
//! ```

#![no_std]
#![feature(type_alias_impl_trait)]

use core::time::Duration;

use px4::{Args, info, main, panic_handler, sleep, task};

panic_handler!();

#[task(wq = "lp_default")]
async fn ticker() {
    info!("ticker started");
    let mut tick: u64 = 0;
    loop {
        tick = tick.wrapping_add(1);
        info!("hello tick={tick}");
        sleep(Duration::from_secs(1)).await;
    }
}

#[main]
fn main(args: Args) -> Result<(), &'static str> {
    match args.subcommand() {
        Some(b"start") => {
            ticker::try_spawn()
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
        _ => Err("usage: hello_module {start|stop|status}"),
    }
}
