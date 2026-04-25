# px4-rs Roadmap

Phases are numbered in implementation order. Each phase is independently
mergeable and has its own acceptance criteria. Mark items `- [x]` when
complete; move finished phases to `archived/`.

| Phase | Title | Status |
| ----- | ----- | ------ |
| 01 | Workspace scaffold | In Progress |
| 02 | `px4-sys` FFI bindings | Complete |
| 03 | `px4-log` + panic handler | Complete |
| 04 | `px4-workqueue` + `#[task]` macro | Core landed; primitives deferred |
| 05 | `px4-msg-codegen` + `#[px4_message]` macro | Complete |
| 06 | `px4-uorb` typed pub/sub | Complete (single-instance) |
| 07 | CMake integration + first end-to-end module on Pixhawk | Infra complete; firmware-build untested |
| 08 | Examples (hello_module, gyro_watch, multi_task) | Not Started |
| 09 | Host-side mock + unit tests | Substantially landed via 04/06 mocks |
| 10 | Documentation polish + crates.io publish | Not Started |
| 11 | SITL end-to-end test infrastructure | Not Started |
