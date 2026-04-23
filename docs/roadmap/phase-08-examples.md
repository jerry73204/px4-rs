# Phase 08 — Example modules

**Goal**: Three examples that together exercise the full crate surface.

**Status**: Not Started
**Priority**: P1
**Depends on**: Phase 04, Phase 06, Phase 07

## Examples

### 08.1 — `hello_module`

- [ ] Pure `px4-workqueue`, no uORB
- [ ] One `#[task(wq = "lp_default")]` that logs "hello" every second via
      a `Timer`
- [ ] Validates: task scaffold, Timer, logging

### 08.2 — `gyro_watch`

- [ ] `px4-workqueue` + `px4-uorb`, no nano-ros
- [ ] Subscribes `SensorGyro`, publishes `VehicleCommand` on spike
- [ ] Validates: async sub/pub, msg codegen, WQ affinity

### 08.3 — `multi_task`

- [ ] Two `#[task]`s on different WQs communicating via `Notify`
- [ ] Validates: cross-task signaling, independent WQ affinity, split
      responsibilities (the preferred PX4 idiom)

## Acceptance criteria

- [ ] All three build on both host (with mocks) and
      `thumbv7em-none-eabihf`
- [ ] `docs/architecture.md` references each example from the Style A
      section
