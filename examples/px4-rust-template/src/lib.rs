//! Skeleton PX4 module — copy this directory, rename
//! `px4_rust_template` everywhere, and start adding tasks.
//!
//! `#[px4::main]` generates the C entry point
//! (`px4_rust_template_main`) that PX4's shell calls; the generated
//! CMake shim wires it into the firmware. On `start`, the body
//! below spawns a single `#[task]` that prints a hello message
//! and resolves.

#![no_std]
#![feature(type_alias_impl_trait)]

use px4::{Args, info, main, panic_handler, task};

panic_handler!();

#[task(wq = "lp_default")]
async fn hello() {
    info!("hello from a Rust PX4 module");
}

#[main]
fn main(args: Args) -> Result<(), &'static str> {
    match args.subcommand() {
        Some(b"start") => {
            hello::try_spawn().map_err(|_| "already running")?.forget();
            Ok(())
        }
        // Skeleton: `stop` / `status` are placeholders. Replace with
        // real teardown / introspection once your module has state.
        Some(b"stop") | Some(b"status") => Ok(()),
        _ => Err("usage: px4_rust_template {start|stop|status}"),
    }
}
