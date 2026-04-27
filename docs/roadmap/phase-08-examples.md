# Phase 08 — Example modules

**Goal**: Three examples that together exercise the full crate surface.

**Status**: Done
**Priority**: P1
**Depends on**: Phase 04, Phase 06, Phase 07

## Examples

### 08.1 — `hello_module`

- [x] Pure `px4-workqueue`, no uORB
- [x] One `#[task(wq = "lp_default")]` that logs `hello tick=N`
      every second via `px4_workqueue::sleep`
- [x] Validates: `#[task]` scaffold, `Sleep`, `px4_log::info!`

### 08.2 — `gyro_watch`

- [x] `px4-workqueue` + `px4-uorb`, no nano-ros
- [x] Subscribes `sensor_gyro`, publishes a custom `gyro_alert`
      message on each magnitude spike. (The original spec named
      `VehicleCommand` as the publish topic; substituted a
      purpose-built `GyroAlert` with three fields because
      `VehicleCommand.msg` ships ~200 lines of constants and would
      drown the example. Same code paths exercised either way:
      `Subscription::recv()`, `Publication::publish()`, the
      `#[px4_message]` codegen for both halves.)
- [x] Validates: async sub/pub, msg codegen, WQ affinity

### 08.3 — `multi_task`

- [x] Two `#[task]`s on different WQs (`hp_default` producer +
      `lp_default` consumer) coordinated by a `Notify`
- [x] Validates: cross-task signaling via `Notify`, independent WQ
      affinity, split responsibilities (the preferred PX4 idiom)

## Acceptance criteria

- [x] All three build on both host and `thumbv7em-none-eabihf` —
      enforced by the `cross-build` job in
      `.github/workflows/ci.yml`, which walks `examples/*/` and
      `cargo build --release --target $TARGET`s every directory
      that ships a `Cargo.toml`.
- [x] `docs/architecture.md` references each example from the
      Style A section.
