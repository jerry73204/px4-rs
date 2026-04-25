# px4-rs — Rust async framework for PX4 Autopilot
#
# Most recipes run on the host (unit tests, codegen, docs). Target builds
# are driven by PX4's CMake via `EXTERNAL_MODULES_LOCATION` — see
# docs/linking-into-px4.md.

set shell := ["bash", "-uc"]
set dotenv-load := true

# Path to the PX4-Autopilot checkout used for codegen + integration tests.
# Override via env or .env file.
export PX4_AUTOPILOT_DIR := env_var_or_default("PX4_AUTOPILOT_DIR", justfile_directory() + "/../PX4-Autopilot")

default:
    @just --list

# ---------------------------------------------------------------------------
# Setup / diagnostics
# ---------------------------------------------------------------------------

# Install rustup toolchain components listed in rust-toolchain.toml.
setup:
    rustup show active-toolchain
    rustup component add rustfmt clippy rust-src
    @echo "PX4_AUTOPILOT_DIR = $PX4_AUTOPILOT_DIR"
    @test -d "$PX4_AUTOPILOT_DIR" \
        || echo "WARNING: PX4_AUTOPILOT_DIR does not exist — codegen recipes will fail"

# Read-only check of the dev environment.
doctor:
    @cargo --version
    @rustc --version
    @echo "PX4_AUTOPILOT_DIR = $PX4_AUTOPILOT_DIR"
    @test -d "$PX4_AUTOPILOT_DIR" \
        && echo "  -> PX4 checkout found" \
        || { echo "  -> MISSING"; exit 1; }
    @command -v bindgen >/dev/null \
        && echo "bindgen-cli installed" \
        || echo "bindgen-cli NOT installed (optional, build.rs uses library API)"

# ---------------------------------------------------------------------------
# Quality
# ---------------------------------------------------------------------------

check: fmt-check clippy

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# ---------------------------------------------------------------------------
# Build / test
# ---------------------------------------------------------------------------

build:
    cargo build --workspace --all-targets

# Build for a bare-metal target. Host test builds don't link the real
# px4-sys — target builds do.
build-target TARGET:
    cargo build --workspace --target {{TARGET}} --exclude xtask

test:
    cargo test --workspace --all-targets

# End-to-end SITL tests (boots `px4` as a subprocess, drives the
# daemon with shell commands). Runs serially via nextest's `sitl`
# test-group; each test is isolated with its own daemon.
#
# Requires: PX4_AUTOPILOT_DIR set, gcc/g++ for the cc-crate compile
# of px4-sys/wrapper.cpp, and `cargo nextest` installed (cargo
# install cargo-nextest --locked).
test-sitl:
    cd tests/sitl && cargo nextest run

# ---------------------------------------------------------------------------
# Docs
# ---------------------------------------------------------------------------

doc:
    cargo doc --workspace --no-deps

doc-open:
    cargo doc --workspace --no-deps --open

# ---------------------------------------------------------------------------
# Codegen
# ---------------------------------------------------------------------------

# Regenerate Rust bindings for every msg in $PX4_AUTOPILOT_DIR/msg/.
gen-msgs:
    cargo run -p xtask -- gen-msgs --px4 "$PX4_AUTOPILOT_DIR"

# Regenerate px4-sys bindgen output. Normally driven by build.rs;
# this recipe is for reviewing the generated file by hand.
gen-sys:
    cargo run -p xtask -- gen-sys --px4 "$PX4_AUTOPILOT_DIR"

# ---------------------------------------------------------------------------
# CI meta
# ---------------------------------------------------------------------------

ci: check test doc
