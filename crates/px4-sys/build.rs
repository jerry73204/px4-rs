//! px4-sys build script.
//!
//! Responsibilities:
//!   1. Generate Rust bindings from `wrapper.h` via bindgen. If bindgen
//!      fails (e.g. no libclang on docs.rs), fall back to the vendored
//!      `bindings/bindings.rs` snapshot.
//!   2. When `PX4_AUTOPILOT_DIR` is set, verify the tree is PX4 >= v1.15.
//!   3. When `PX4_RS_BUILD_TRAMPOLINES` is also set (done by PX4's CMake
//!      `px4_rust_module()` helper, never by a plain `cargo build`),
//!      compile `wrapper.cpp` against the real PX4 headers. That path
//!      requires the CMake-generated `px4_boardconfig.h`, so it only
//!      works inside a real PX4 build invocation.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MIN_PX4_MARKER_FILE: &str = "platforms/common/uORB/uORB.h";
// `orb_id_size_t` is a typedef introduced exactly at v1.15 and lives in the
// field declaration of orb_metadata.o_id. Safer than grepping for the
// word "message_hash" which can appear in comments/changelogs.
const MIN_PX4_MARKER: &str = "orb_id_size_t";

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=wrapper.cpp");
    println!("cargo:rerun-if-env-changed=PX4_AUTOPILOT_DIR");
    println!("cargo:rerun-if-env-changed=PX4_RS_BUILD_TRAMPOLINES");

    generate_bindings(&crate_dir, &out_dir);

    let Some(px4_dir) = env::var_os("PX4_AUTOPILOT_DIR").map(PathBuf::from) else {
        return;
    };

    verify_px4_version(&px4_dir);

    // The C++ trampolines need PX4's full include hierarchy including the
    // CMake-generated `px4_boardconfig.h`. Plain `cargo build` with just
    // PX4_AUTOPILOT_DIR set doesn't have that — only a PX4 CMake-driven
    // build does. CMake's `px4_rust_module()` sets this flag to opt in.
    if env::var_os("PX4_RS_BUILD_TRAMPOLINES").is_some() {
        compile_trampolines(&crate_dir, &px4_dir);
    }
}

/// Try bindgen; on failure, copy the vendored snapshot. Either way,
/// `$OUT_DIR/bindings.rs` exists after this returns.
fn generate_bindings(crate_dir: &Path, out_dir: &Path) {
    let out_file = out_dir.join("bindings.rs");
    let snapshot = crate_dir.join("bindings").join("bindings.rs");
    let header = crate_dir.join("wrapper.h");

    let result = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .use_core()
        .generate_comments(false)
        .layout_tests(false)
        .prepend_enum_name(false)
        .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .allowlist_function("hrt_.*")
        .allowlist_function("orb_.*")
        .allowlist_function("px4_log_.*")
        .allowlist_function("px4_rs_.*")
        .allowlist_type("hrt_.*")
        .allowlist_type("orb_.*")
        .allowlist_type("px4_rs_.*")
        .allowlist_var("PX4_RS_LOG_LEVEL_.*")
        .generate();

    match result {
        Ok(bindings) => bindings
            .write_to_file(&out_file)
            .expect("write OUT_DIR/bindings.rs"),
        Err(e) => {
            println!(
                "cargo:warning=bindgen failed ({e}); falling back to vendored snapshot at {}",
                snapshot.display()
            );
            if !snapshot.exists() {
                panic!(
                    "bindgen failed and no vendored snapshot at {}. \
                     Run `just gen-sys` once in a working-bindgen environment.",
                    snapshot.display()
                );
            }
            fs::copy(&snapshot, &out_file).expect("copy vendored bindings snapshot");
        }
    }
}

/// Hard-fail the build if `PX4_AUTOPILOT_DIR` points at a pre-v1.15 tree.
/// Detection: `platforms/common/uORB/uORB.h` must contain `message_hash`.
fn verify_px4_version(px4_dir: &Path) {
    let marker = px4_dir.join(MIN_PX4_MARKER_FILE);
    let body = fs::read_to_string(&marker).unwrap_or_else(|e| {
        panic!(
            "PX4_AUTOPILOT_DIR = {}: cannot read {}: {e}. Is this a PX4 source tree?",
            px4_dir.display(),
            marker.display()
        )
    });

    if !body.contains(MIN_PX4_MARKER) {
        panic!(
            "PX4_AUTOPILOT_DIR = {}: detected pre-v1.15 PX4 \
             (uORB.h has no `orb_id_size_t` typedef). \
             px4-sys requires PX4 >= v1.15.0. \
             See docs/roadmap/phase-02-px4-sys.md.",
            px4_dir.display()
        );
    }

    println!("cargo:rerun-if-changed={}", marker.display());
}

/// Compile `wrapper.cpp` against the real PX4 headers. The static_asserts
/// inside will fail the compile if any struct layout drifted.
fn compile_trampolines(crate_dir: &Path, px4_dir: &Path) {
    let includes = [
        "platforms/common/include",
        "platforms/common",
        "src",
        "src/lib",
        "src/include",
        "src/modules",
        "msg",
    ];

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file(crate_dir.join("wrapper.cpp"))
        .include(crate_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-address-of-packed-member")
        .define("__PX4_POSIX", None);

    for inc in includes {
        let p = px4_dir.join(inc);
        if p.is_dir() {
            build.include(p);
        }
    }

    // If any of the headers we include change under the PX4 tree, rebuild.
    for rel in [
        "platforms/common/uORB/uORB.h",
        "platforms/common/uORB/SubscriptionCallback.hpp",
        "platforms/common/include/px4_platform_common/px4_work_queue/WorkItem.hpp",
        "platforms/common/include/px4_platform_common/px4_work_queue/ScheduledWorkItem.hpp",
        "platforms/common/include/px4_platform_common/px4_work_queue/WorkQueueManager.hpp",
        "src/drivers/drv_hrt.h",
    ] {
        println!("cargo:rerun-if-changed={}", px4_dir.join(rel).display());
    }

    build.compile("px4_rs_wrapper");
}
