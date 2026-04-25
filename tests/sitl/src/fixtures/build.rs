//! Cached PX4 SITL build.
//!
//! `make px4_sitl EXTERNAL_MODULES_LOCATION=…` is expensive (cold:
//! ~60 seconds; warm: ~2 seconds for ninja "no work to do"). Each
//! test process invokes [`ensure_built`] from its `Px4Sitl::boot`,
//! and the work runs at most once per process via `OnceLock`.
//!
//! Cross-process serialization (multiple `cargo nextest` worker
//! processes racing on the same `make` invocation) is **not** handled
//! here — the `sitl` test-group in `.config/nextest.toml` caps that
//! group at 1 thread, so only one process is ever in this code path
//! at a time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::{Result, TestError};

/// Returns the absolute path to the PX4 build directory containing
/// `bin/px4`. Triggers a build on first call.
pub fn ensure_built() -> Result<PathBuf> {
    static CACHED: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();
    CACHED
        .get_or_init(|| build().map_err(|e| e.to_string()))
        .clone()
        .map_err(TestError::BuildFailed)
}

fn build() -> Result<PathBuf> {
    let px4 = px4_source_dir()?;
    let externals = externals_dir()?;

    let status = Command::new("make")
        .arg("px4_sitl")
        .arg(format!(
            "EXTERNAL_MODULES_LOCATION={}",
            externals.display()
        ))
        .current_dir(&px4)
        // PX4's make wrapper prints a fair amount; let it stream so
        // failures are debuggable. The OnceLock means this only runs
        // once per test process anyway.
        .status()
        .map_err(|e| TestError::BuildFailed(format!("spawn make: {e}")))?;

    if !status.success() {
        return Err(TestError::BuildFailed(format!(
            "`make px4_sitl EXTERNAL_MODULES_LOCATION={}` exited {}",
            externals.display(),
            status.code().unwrap_or(-1)
        )));
    }

    let build_dir = px4.join("build").join("px4_sitl_default");
    let bin = build_dir.join("bin").join("px4");
    if !bin.is_file() {
        return Err(TestError::BuildFailed(format!(
            "expected {} after build, but it's missing",
            bin.display()
        )));
    }
    Ok(build_dir)
}

/// Resolve `PX4_AUTOPILOT_DIR`. Errors if unset or non-existent.
pub fn px4_source_dir() -> Result<PathBuf> {
    let raw = std::env::var_os("PX4_AUTOPILOT_DIR").ok_or(TestError::NoPx4Tree)?;
    let p = PathBuf::from(raw);
    if !p.is_dir() {
        return Err(TestError::NoPx4Tree);
    }
    Ok(p)
}

/// Path to the `tests/sitl/px4-externals` tree shipped with this
/// crate. Computed from `CARGO_MANIFEST_DIR` so it works regardless
/// of where cargo was invoked.
pub fn externals_dir() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = manifest.join("px4-externals");
    if !p.join("src").join("CMakeLists.txt").is_file() {
        return Err(TestError::BuildFailed(format!(
            "{} missing — set up px4-externals/src/CMakeLists.txt first",
            p.display()
        )));
    }
    Ok(p)
}

/// Test-only entry: short-circuits `ensure_built` to a pre-existing
/// build directory. Useful when a developer already has SITL built
/// and wants to skip the redundant make step.
#[allow(dead_code)]
pub fn build_dir_override() -> Option<PathBuf> {
    std::env::var_os("PX4_RS_SITL_BUILD_DIR").map(PathBuf::from)
}

/// Defensively check that a path looks like a SITL build dir. Used by
/// `Px4Sitl` before exec'ing the binary so we get a clearer error
/// than "command not found".
#[allow(dead_code)]
pub fn looks_like_sitl_build(p: &Path) -> bool {
    p.join("bin").join("px4").is_file() && p.join("etc").join("init.d-posix").join("rcS").is_file()
}
