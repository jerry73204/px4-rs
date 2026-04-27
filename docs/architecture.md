# Architecture

## Why px4-rs exists

PX4's `WorkQueue` is already a cooperative scheduler: a dedicated pthread
drains a FIFO of `WorkItem`s, each woken by `ScheduleNow()`. Rust's async
model has exactly the same shape — tasks, a ready-queue, a waker that
enqueues tasks. px4-rs closes that identity:

```
Rust async                PX4
──────────                ───
Task                ≡     WorkItem
Waker::wake()       →     ScheduleNow()
ready queue         ≡     WorkQueue::_q
poll()              ≡     WorkItem::Run()
executor thread     ≡     WorkQueue pthread
```

No second runtime is layered on top. A `#[task]` expands to a `ScheduledWorkItem`
subclass whose `Run()` polls the future exactly once per wake. This is the
1:1 mapping — not the embassy-style "N tasks / 1 executor thread" model.

## Crate graph

```
         px4-sys
            │
     ┌──────┴──────────────┐
     │                     │
  px4-log           px4-workqueue-macros
     │                     │
     ├──────────────┐      │
     │              ▼      ▼
     │         px4-workqueue
     │              │
     │              ▼
     │          px4-uorb ◄──── px4-msg-macros ◄── px4-msg-codegen
     │              │
     ▼              ▼
   (user PX4 modules) — Style A
                  ▲
                  │
          nros-rmw-uorb  (in nano-ros repo) — enables Style B + Style C
```

### Layering rules

- `px4-sys` has zero dependencies on other px4-rs crates and is the only
  place `unsafe extern "C"` bindings to PX4 live.
- `px4-workqueue` never depends on `px4-uorb`. uORB is one wake source among
  several; cross-task `Notify`, `Timer` (hrt), and GPIO-IRQ wakers sit at the
  same layer.
- `px4-uorb` depends on `px4-workqueue` only for the waker slot type
  (`AtomicWaker`) and the `ScheduleNow` FFI handle — not for `#[task]`. A
  synchronous, non-async uORB user should still be able to `use px4-uorb`.
- `px4-msg-codegen` is `std`-only (runs at build time). `px4-msg-macros`
  re-exports it as a proc-macro. No runtime crate pulls either.

## Three usage styles

### Style A — standalone PX4 module

```rust
#[task(wq = "rate_ctrl")]
async fn rate_limit(mut g: Subscription<SensorGyro>,
                    p: Publication<ActuatorControls>) -> ! {
    loop {
        let s = g.recv().await;
        p.publish(&clamp(s));
    }
}
```

Depends on: `px4-workqueue`, `px4-uorb`, `px4-msg-macros`. No nano-ros.

Three Style-A reference modules ship under `examples/`, each one
exercising a different slice of the runtime:

- [`examples/hello_module/`](../examples/hello_module/) — the smallest
  thing that can be a PX4 Rust module. One `#[task]` on `lp_default`
  that prints once a second via `px4_workqueue::sleep`. No uORB.
- [`examples/multi_task/`](../examples/multi_task/) — two `#[task]`s
  on different WorkQueues (a producer on `hp_default`, a consumer on
  `lp_default`) coordinated by a single `Notify`. Demonstrates the
  preferred PX4 idiom of putting time-driven nudging on a separate WQ
  thread from the actual work.
- [`examples/gyro_watch/`](../examples/gyro_watch/) — subscribes to
  `sensor_gyro` and publishes a custom `gyro_alert` whenever the
  rotation magnitude crosses a threshold. Exercises `Subscription` +
  `Publication` + `#[px4_message]` codegen in one short task body.
- [`examples/heartbeat/`](../examples/heartbeat/) — earlier
  phase-07 reference for raw `Publication` use; kept around as the
  smallest pub-only module.

### Style B — nano-ros callback API on PX4

Executor lives inside a single `NrosWorkItem`; `Run()` calls
`executor.spin_once(Duration::ZERO)`. Every nano-ros feature (timers,
services, actions, params, lifecycle) works unchanged because the arena
is pumped by the WorkQueue thread.

Depends on: `nros-rmw-uorb` (in nano-ros), which depends on `px4-uorb` here.

### Style C — `async fn` with nano-ros topic naming

Same `#[task]` macro as Style A; the future body uses `nros::Node` for
ROS 2 topic names and typed ROS messages.

Depends on: `nros-rmw-uorb` + `px4-workqueue`. Feature-limited to
primitives with async wakers (pub/sub/timer). Services/actions/lifecycle
fall back to a parallel Style-B `NrosWorkItem`.

## What lives in px4-rs vs. nano-ros

| Concern | px4-rs | nano-ros |
| --- | --- | --- |
| Raw uORB FFI | ✔ (`px4-sys`) | — |
| Typed uORB pub/sub | ✔ (`px4-uorb`) | — |
| `#[task]` + waker | ✔ (`px4-workqueue`) | — |
| PX4 msg codegen | ✔ (`px4-msg-codegen`) | — |
| ROS 2 name → uORB topic map | — | ✔ (`nros-rmw-uorb`) |
| ROS message codegen | — | ✔ (`nros-codegen`) |
| ROS 2 QoS / executor traits | — | ✔ (`nros-rmw`, `nros-node`) |

The two projects sit side-by-side; nano-ros depends on px4-rs, never the
reverse.

## See also

- [async-model.md](async-model.md) — waker chain, polling flow, what
  happens on a uORB publish
- [task-macro.md](task-macro.md) — `#[task]` expansion contract
- [linking-into-px4.md](linking-into-px4.md) — how PX4 consumes the
  built staticlib
