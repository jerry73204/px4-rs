//! End-to-end macro expansion test. Uses a small fixture file that
//! lives inside this crate so the test has no external dependency on
//! a PX4 checkout.

use px4_msg_macros::px4_message;

#[px4_message("tests/fixtures/SensorGyro.msg")]
pub struct SensorGyro;

#[test]
fn struct_has_expected_layout() {
    // Layout rule: u64 × 2, u32 × 6, u8 × 4, tail pad to 48.
    assert_eq!(core::mem::size_of::<SensorGyro>(), 48);

    // Access a field to verify the struct is actually usable.
    let s = SensorGyro {
        timestamp: 1,
        timestamp_sample: 2,
        device_id: 3,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        temperature: 0.0,
        error_count: 0,
        clip_counter: [0; 3],
        samples: 0,
        _padding0: [0; 4],
    };
    assert_eq!(s.timestamp, 1);
    assert_eq!(SensorGyro::ORB_QUEUE_LENGTH, 8);
    assert_eq!(SensorGyro::TOPICS, ["sensor_gyro"]);
}
