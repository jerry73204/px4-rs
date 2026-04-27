# Phase 01 — Workspace scaffold

**Goal**: Land the empty workspace with a working `just ci` on the host.

**Status**: Done
**Priority**: P0 (unblocks everything)
**Depends on**: nothing

## Work items

- [x] 01.1 — Workspace `Cargo.toml` with resolver 3 and `[workspace.dependencies]`
- [x] 01.2 — `rust-toolchain.toml` pinning a nightly with tier-3 NuttX targets
- [x] 01.3 — `.cargo/config.toml` with target rustflags
- [x] 01.4 — Root `README.md`, `LICENSE-BSD-3`, `.gitignore`
- [x] 01.5 — `justfile` with `setup`, `doctor`, `check`, `build`, `test`, `doc`, `ci`
- [x] 01.6 — `docs/` skeleton (`architecture.md`, `async-model.md`, `task-macro.md`, `linking-into-px4.md`)
- [x] 01.7 — `docs/roadmap/` with this document and the index
- [x] 01.8 — `xtask/` scaffold landed in phase 02 alongside `gen-sys`
      and grew `gen-msgs` in phase 05; the `xtask` recipe set is now
      the canonical home for codegen helpers.
- [x] 01.9 — CI (GitHub Actions) — `.github/workflows/ci.yml` runs
      `just ci`, cross-builds for all three Rust targets we claim
      support for, and gates the px4-sys snapshot against PX4 v1.16.2.

## Acceptance criteria

- [x] `git clone && just setup && just ci` green on a fresh Ubuntu
      runner — covered by the `ci` job in `.github/workflows/ci.yml`.
- [x] `cargo doc --workspace --no-deps` produces output — runs as part
      of `just ci`'s `doc` step.
- [x] `just doctor` prints a useful message when `PX4_AUTOPILOT_DIR`
      is missing — recipe is in `justfile`.

## Notes

- Leave `[workspace.members]` commented until each crate has real content.
  Cargo refuses to build a workspace that lists non-existent members.
- `xtask` exists only so CI has something to build. Real tool
  implementations land in later phases.
