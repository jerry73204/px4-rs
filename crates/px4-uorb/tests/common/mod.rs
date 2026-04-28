//! Shared fixtures for the phase-06.6 extension tests. Each test
//! file lives in its own binary, so we factor the message + helpers
//! here and `mod common` from each.

#![allow(dead_code)]

use px4_msg_macros::px4_message;

#[px4_message("tests/fixtures/SensorGyro.msg")]
pub struct SensorGyro;

pub fn sample(stamp: u32) -> SensorGyro {
    SensorGyro {
        timestamp: stamp as u64,
        timestamp_sample: stamp as u64,
        device_id: stamp,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        temperature: 0.0,
        error_count: 0,
        clip_counter: [0; 3],
        samples: 0,
        _padding0: [0; 4],
    }
}

// `yield_now` lives in `px4_workqueue` now. Re-export so the
// extension test files keep working without each importing it.
pub use px4_workqueue::yield_now;
