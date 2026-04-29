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
    println!("cargo:rustc-check-cfg=cfg(px4_rs_trampolines)");

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
        // Signal to lib.rs that real trampolines are linked in, so it
        // skips the Rust-side stubs that would otherwise collide.
        println!("cargo:rustc-cfg=px4_rs_trampolines");
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
///
/// Include / define plumbing:
///
/// - `PX4_AUTOPILOT_DIR` — PX4 source tree.
/// - `PX4_RS_BUILD_DIR` — PX4 CMake build dir. Holds the
///   CMake-generated `px4_boardconfig.h` and the `uORB/topics/*.h`
///   headers. Set by `px4_rust_module()`.
/// - `PX4_RS_PLATFORM` — "posix" or "nuttx". Drives
///   `-D__PX4_POSIX` / `-D__PX4_NUTTX` + the platform-specific
///   include subtree.
fn compile_trampolines(crate_dir: &Path, px4_dir: &Path) {
    let platform = env::var("PX4_RS_PLATFORM").unwrap_or_else(|_| "nuttx".to_string());
    let build_dir = env::var_os("PX4_RS_BUILD_DIR").map(PathBuf::from);

    let common_includes = [
        "platforms/common/include",
        "platforms/common",
        "src",
        "src/lib",
        "src/lib/matrix",
        "src/include",
        "src/modules",
        "msg",
    ];
    let posix_includes = [
        "platforms/posix/include",
        "platforms/posix/src/px4/common/include",
        "platforms/posix/src/px4/generic/generic/include",
        "boards/px4/sitl/src",
    ];

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("gnu++17")
        .file(crate_dir.join("wrapper.cpp"))
        .include(crate_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-address-of-packed-member");

    // Platform-specific preprocessor defines + include subtree.
    match platform.as_str() {
        "posix" => {
            build
                .define("__PX4_POSIX", None)
                .define("__PX4_LINUX", None)
                .define("CONFIG_ARCH_BOARD_PX4_SITL", None)
                .define("ENABLE_LOCKSTEP_SCHEDULER", None)
                .define("__STDC_FORMAT_MACROS", None)
                .define("__CUSTOM_FILE_IO__", None)
                .define("noreturn_function", "__attribute__((noreturn))")
                .define("MODULE_NAME", "\"px4_rs_wrapper\"")
                // PX4's build forcibly includes visibility.h at the
                // start of every TU; replicate so the __EXPORT /
                // __BEGIN_DECLS macros resolve.
                .flag("-includevisibility.h")
                // Match the NuttX branch's no-exceptions stance so a
                // bad `new` returns nullptr on both platforms — the
                // wrapper relies on that.
                .flag("-fno-exceptions");
            for inc in posix_includes {
                let p = px4_dir.join(inc);
                if p.is_dir() {
                    build.include(p);
                }
            }
        }
        _ => {
            build
                .define("__PX4_NUTTX", None)
                .define("__STDC_FORMAT_MACROS", None)
                .define("__CUSTOM_FILE_IO__", None)
                .define("_SYS_CDEFS_H_", None)
                .define("_SYS_REENT_H_", None)
                .define("NDEBUG", None)
                .define("MODULE_NAME", "\"px4_rs_wrapper\"")
                .flag("-includevisibility.h")
                // PX4 builds NuttX modules with a curated subset of
                // libstdc++ — `-nostdinc++` drops the toolchain's
                // newlib c++ tree so only NuttX's `cxx/` headers
                // (via `-isystem` below) resolve. Without it, NuttX's
                // and newlib's `<cmath>` mix and `NAN`/`std::nothrow`
                // come out undefined. The `-f...` flags match PX4's
                // common cxx options (no exceptions/RTTI etc.).
                .flag("-nostdinc++")
                .flag("-fno-exceptions")
                .flag("-fno-rtti")
                .flag("-fno-sized-deallocation")
                .flag("-fno-threadsafe-statics")
                .flag("-fcheck-new");
            // CONFIG_ARCH_BOARD_<UPPERCASE> mirrors what PX4's build
            // sets; some headers gate behaviour on it.
            if let Ok(name) = env::var("PX4_RS_BOARD_NAME") {
                build.define(&format!("CONFIG_ARCH_BOARD_{name}"), None);
            }
            for inc in nuttx_includes(&px4_dir, build_dir.as_deref()) {
                if inc.is_dir() {
                    build.include(inc);
                }
            }
            // System-include trio that brings NuttX's `<sys/...>`
            // headers (ioctl.h, etc.) into scope before the bare-metal
            // toolchain's missing/broken ones. `cc` doesn't have a
            // dedicated `-isystem` API so we splice the flags
            // manually.
            for sys_inc in nuttx_system_includes(&px4_dir) {
                if sys_inc.is_dir() {
                    build.flag("-isystem").flag(sys_inc.to_str().unwrap());
                }
            }
            println!("cargo:rerun-if-env-changed=PX4_RS_BOARD_NAME");
            println!("cargo:rerun-if-env-changed=PX4_RS_BOARD_DIR");
            println!("cargo:rerun-if-env-changed=PX4_RS_CHIP");
            println!("cargo:rerun-if-env-changed=PX4_RS_ARCH_FAMILY");
        }
    }

    for inc in common_includes {
        let p = px4_dir.join(inc);
        if p.is_dir() {
            build.include(p);
        }
    }

    // The CMake build dir holds px4_boardconfig.h and the generated
    // uORB/topics/*.h headers. Without it, wrapper.cpp can't include
    // px4_platform_common/defines.h.
    if let Some(bd) = &build_dir {
        build.include(bd);
        build.include(bd.join("src").join("lib"));
    } else {
        println!(
            "cargo:warning=PX4_RS_BUILD_DIR unset — wrapper.cpp will likely fail to find px4_boardconfig.h"
        );
    }

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
    println!("cargo:rerun-if-env-changed=PX4_RS_PLATFORM");
    println!("cargo:rerun-if-env-changed=PX4_RS_BUILD_DIR");

    build.compile("px4_rs_wrapper");
}

/// Regular `-I` paths for a NuttX build. `cc` adds them in order; the
/// later entries don't override earlier ones, so list the most
/// specific paths first.
///
/// `PX4_RS_CHIP` (e.g. `stm32h7`), `PX4_RS_ARCH_FAMILY` (e.g.
/// `armv7-m`), and `PX4_RS_BOARD_DIR` come from `px4_rust_module()`'s
/// CMake env. Without them we fall back to safe defaults — the build
/// will then likely fail on a missing header, but at least cleanly.
fn nuttx_includes(px4_dir: &Path, build_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(board_dir) = env::var("PX4_RS_BOARD_DIR") {
        paths.push(PathBuf::from(board_dir).join("src"));
    }
    paths.push(px4_dir.join("platforms/nuttx/src/px4/common/include"));
    if let Ok(chip) = env::var("PX4_RS_CHIP") {
        // chip values look like `stm32h7`, `stm32f7` — the path
        // splits the family out of the chip prefix.
        let (vendor, _) = chip.split_at(chip.find(char::is_numeric).unwrap_or(chip.len()));
        paths.push(
            px4_dir
                .join("platforms/nuttx/src/px4")
                .join(vendor)
                .join(&chip)
                .join("include"),
        );
    }
    if let Ok(arch) = env::var("PX4_RS_ARCH_FAMILY") {
        paths.push(
            px4_dir
                .join("platforms/nuttx/NuttX/nuttx/arch/arm/src")
                .join(&arch),
        );
    }
    paths.push(px4_dir.join("platforms/nuttx/NuttX/nuttx/arch/arm/src/chip"));
    paths.push(px4_dir.join("platforms/nuttx/NuttX/nuttx/arch/arm/src/common"));
    paths.push(px4_dir.join("platforms/nuttx/NuttX/apps/include"));

    if let Some(bd) = build_dir {
        paths.push(bd.join("external/Install/include"));
    }
    paths
}

/// `-isystem` paths for a NuttX build. These take precedence over the
/// bare-metal toolchain's defaults so NuttX's `<sys/ioctl.h>`,
/// `<cstdio>`, etc. resolve before the empty / wrong arm-none-eabi
/// counterparts.
fn nuttx_system_includes(px4_dir: &Path) -> Vec<PathBuf> {
    vec![
        px4_dir.join("platforms/nuttx/NuttX/include/cxx"),
        px4_dir.join("platforms/nuttx/NuttX/nuttx/include/cxx"),
        px4_dir.join("platforms/nuttx/NuttX/nuttx/include"),
    ]
}
