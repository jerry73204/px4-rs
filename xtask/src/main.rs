//! px4-rs xtask — codegen helpers invoked from the `just` recipes.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let cmd = match args.next() {
        Some(c) => c,
        None => {
            usage();
            return ExitCode::from(2);
        }
    };
    let rest: Vec<String> = args.collect();

    let result = match cmd.as_str() {
        "gen-sys" => gen_sys(&rest),
        "gen-msgs" => gen_msgs(&rest),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => Err(format!("unknown subcommand: {other}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "px4-rs xtask\n\
         \n\
         Subcommands:\n\
         \x20 gen-sys  [--px4 DIR]   regenerate crates/px4-sys/bindings/bindings.rs\n\
         \x20 gen-msgs [--px4 DIR]   (phase 05) regenerate uORB message bindings"
    );
}

fn parse_px4_flag(rest: &[String]) -> Option<PathBuf> {
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--px4" {
            return it.next().map(PathBuf::from);
        }
    }
    env::var_os("PX4_AUTOPILOT_DIR").map(PathBuf::from)
}

fn workspace_root() -> PathBuf {
    // xtask lives at <root>/xtask. CARGO_MANIFEST_DIR points there.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask parent")
        .to_path_buf()
}

fn gen_sys(rest: &[String]) -> Result<(), String> {
    let root = workspace_root();
    let crate_dir = root.join("crates").join("px4-sys");
    let wrapper = crate_dir.join("wrapper.h");
    let out = crate_dir.join("bindings").join("bindings.rs");

    if let Some(px4) = parse_px4_flag(rest) {
        // Optional: sanity-check version, same rule as build.rs.
        let marker = px4.join("platforms/common/uORB/uORB.h");
        match fs::read_to_string(&marker) {
            Ok(body) if body.contains("orb_id_size_t") => {
                println!("PX4 >= v1.15 detected at {}", px4.display());
            }
            Ok(_) => {
                return Err(format!(
                    "{}: pre-v1.15 PX4 tree (no orb_id_size_t in uORB.h)",
                    px4.display()
                ));
            }
            Err(e) => eprintln!(
                "warning: could not read {}: {e}. Skipping PX4 version check.",
                marker.display()
            ),
        }
    }

    println!(
        "regenerating bindings from {} into {}",
        wrapper.display(),
        out.display()
    );

    run_bindgen(&wrapper, &out)?;

    println!("ok. Remember to `cargo fmt` if desired.");
    Ok(())
}

fn run_bindgen(header: &Path, out: &Path) -> Result<(), String> {
    let bindings = bindgen::Builder::default()
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
        .generate()
        .map_err(|e| format!("bindgen: {e}"))?;

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    bindings
        .write_to_file(out)
        .map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(())
}

fn gen_msgs(_rest: &[String]) -> Result<(), String> {
    Err("gen-msgs is implemented in phase 05 (px4-msg-codegen)".to_string())
}
