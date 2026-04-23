//! Cross-check against the pinned PX4 tree (v1.16.2). Skipped if
//! `PX4_AUTOPILOT_DIR` isn't set so CI can still run without a PX4
//! checkout.

use std::path::PathBuf;

use px4_msg_codegen::{Resolver, parse_file};

fn px4_dir() -> Option<PathBuf> {
    std::env::var_os("PX4_AUTOPILOT_DIR").map(PathBuf::from)
}

#[test]
fn every_msg_parses_and_lays_out() {
    let Some(px4) = px4_dir() else {
        eprintln!("skipped: PX4_AUTOPILOT_DIR not set");
        return;
    };
    let msg_dir = px4.join("msg");
    if !msg_dir.is_dir() {
        eprintln!("skipped: {}/msg missing", px4.display());
        return;
    }

    let mut resolver = Resolver::new(vec![msg_dir.clone()]);
    let mut count = 0usize;
    for entry in std::fs::read_dir(&msg_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("msg") {
            continue;
        }
        let def = parse_file(&path)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        resolver
            .layout(&def)
            .unwrap_or_else(|e| panic!("layout {}: {e}", path.display()));
        count += 1;
    }
    assert!(count > 100, "expected lots of PX4 messages, got {count}");
    eprintln!("parsed + laid out {count} messages");
}

#[test]
fn sensor_gyro_size_cross_check() {
    let Some(px4) = px4_dir() else { return };
    let path = px4.join("msg").join("SensorGyro.msg");
    if !path.is_file() {
        return;
    }

    let def = parse_file(&path).unwrap();
    let mut resolver = Resolver::new(vec![path.parent().unwrap().to_path_buf()]);
    let laid = resolver.layout(&def).unwrap();

    // PX4's C++ sensor_gyro_s is 48 bytes on all 64-bit-timestamp builds.
    assert_eq!(laid.size, 48, "SensorGyro size drift");
}
