# px4-rs

Rust async framework for PX4 Autopilot modules. Treats the PX4 `WorkQueue` as
a first-class Rust async executor: `Waker::wake()` maps directly to
`WorkItem::ScheduleNow()`, so a single `#[task]` definition compiles to a
native PX4 `ScheduledWorkItem` with no second scheduler on top.

## Crates

| Crate | Purpose |
| ----- | ------- |
| `px4-sys` | Raw `-sys` FFI bindings (bindgen): uORB, WorkQueue, hrt, log |
| `px4-log` | `PX4_INFO` / `PX4_WARN` / `PX4_ERR` shims + `log` adapter |
| `px4-workqueue` | `#[task]` + `WorkItemCell` + `RawWaker` → `ScheduleNow` + `Timer` |
| `px4-workqueue-macros` | Proc-macro crate backing `#[task]` |
| `px4-msg-codegen` | Parser for PX4 `msg/*.msg` files |
| `px4-msg-macros` | `#[px4_message("msg/foo.msg")]` proc-macro |
| `px4-uorb` | Typed `Publication<M>` / `Subscription<M>` over uORB |

## Non-goals

- Not a generic Rust-on-embedded runtime. Use [Embassy](https://embassy.dev)
  or RTIC for non-PX4 targets.
- Not a ROS 2 client. That lives in [nano-ros](../nano-ros) via
  `nros-rmw-uorb`, which depends on this project.

## Getting started

See [docs/architecture.md](docs/architecture.md) for the layering, then
[docs/linking-into-px4.md](docs/linking-into-px4.md) for dropping a built
staticlib into a PX4 firmware build via `EXTERNAL_MODULES_LOCATION`.

## Commands

```
just setup      # install toolchain components + xtask prereqs
just check      # fmt + clippy
just build      # host-side build (unit tests, codegen)
just test       # host unit tests
just doc        # rustdoc for all crates
just gen-msgs   # generate Rust bindings from $PX4_AUTOPILOT_DIR/msg/*.msg
```

## License

BSD-3-Clause, matching PX4.
