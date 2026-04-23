# Phase 10 — Documentation polish + publish

**Goal**: Publish 0.1 to crates.io and announce.

**Status**: Not Started
**Priority**: P2
**Depends on**: all prior phases

## Work items

- [ ] 10.1 — rustdoc pass on every public item (`#![warn(missing_docs)]`)
- [ ] 10.2 — `docs/book/` mdbook with getting-started + crate-by-crate
      reference
- [ ] 10.3 — `CHANGELOG.md` with 0.1.0 entry
- [ ] 10.4 — `cargo publish --dry-run` for each crate in dep order
- [ ] 10.5 — Actually publish — px4-sys, px4-log, px4-workqueue-macros,
      px4-workqueue, px4-msg-codegen, px4-msg-macros, px4-uorb
- [ ] 10.6 — Announce on PX4 Discuss + r/rust

## Acceptance criteria

- [ ] `cargo install px4-rs-template` (or whatever the template ends up
      being) works end-to-end against a real PX4 checkout
- [ ] All crates show green on docs.rs
