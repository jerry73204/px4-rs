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

fn gen_msgs(rest: &[String]) -> Result<(), String> {
    let px4 = parse_px4_flag(rest)
        .ok_or_else(|| "gen-msgs: --px4 <DIR> required (or set PX4_AUTOPILOT_DIR)".to_string())?;
    let msg_dir = px4.join("msg");
    if !msg_dir.is_dir() {
        return Err(format!("{}: no `msg/` subdirectory", px4.display()));
    }

    let out = parse_flag(rest, "--out").unwrap_or_else(|| {
        workspace_root()
            .join("crates")
            .join("px4-msg-codegen")
            .join("generated")
    });
    fs::create_dir_all(&out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;

    let entries =
        fs::read_dir(&msg_dir).map_err(|e| format!("readdir {}: {e}", msg_dir.display()))?;
    let mut count = 0usize;
    let mut skipped = Vec::<(String, String)>::new();

    for entry in entries {
        let entry = entry.map_err(|e| format!("readdir: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("msg") {
            continue;
        }
        match px4_msg_codegen::generate(&path, vec![msg_dir.clone()]) {
            Ok(ts) => {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                let snake = px4_msg_codegen::model::camel_to_snake(stem);
                let out_file = out.join(format!("{snake}.rs"));
                let contents = ts.to_string();
                fs::write(&out_file, contents)
                    .map_err(|e| format!("write {}: {e}", out_file.display()))?;
                count += 1;
            }
            Err(e) => {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                skipped.push((stem, e.to_string()));
            }
        }
    }

    println!("generated {count} messages into {}", out.display());
    if !skipped.is_empty() {
        println!("\n{} messages skipped:", skipped.len());
        for (name, err) in &skipped {
            println!("  - {name}: {err}");
        }
    }
    Ok(())
}

fn parse_flag(rest: &[String], key: &str) -> Option<PathBuf> {
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == key {
            return it.next().map(PathBuf::from);
        }
    }
    None
}
