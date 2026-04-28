# px4-rs Roadmap

Phases are numbered in implementation order. Each phase is independently
mergeable and has its own acceptance criteria. Mark items `- [x]` when
complete; move finished phases to `archived/`.

| Phase | Title | Status |
| ----- | ----- | ------ |
| 01 | Workspace scaffold | Done |
| 02 | `px4-sys` FFI bindings | Done |
| 03 | `px4-log` + panic handler | Done |
| 04 | `px4-workqueue` + `#[task]` macro | Done — primitives + trybuild |
| 05 | `px4-msg-codegen` + `#[px4_message]` macro | Done |
| 06 | `px4-uorb` typed pub/sub | Done — multi-instance + interval knobs |
| 07 | CMake integration + first end-to-end module on Pixhawk | Done — verified by phase-11 SITL suite |
| 08 | Examples (hello_module, gyro_watch, multi_task) | Done |
| 09 | Host-side mock + unit tests | Done — multi-OS CI matrix |
| 10 | Documentation polish + crates.io publish | Not Started |
| 11 | SITL end-to-end test infrastructure | Done — 12 tests warm |
| 12 | `px4` umbrella crate + `#[px4::main]` | Done |
| 13 | Renode + NuttX e2e test track | Not Started |
