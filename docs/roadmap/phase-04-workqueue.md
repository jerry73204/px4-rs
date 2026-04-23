# Phase 04 — `px4-workqueue` + `#[task]` macro

**Goal**: Rust async runtime on PX4 WorkQueue, 1 task ≡ 1 WorkItem. This
is the project's signature crate.

**Status**: Not Started
**Priority**: P0
**Depends on**: Phase 02, Phase 03

## Architecture

See [docs/async-model.md](../async-model.md) and
[docs/task-macro.md](../task-macro.md).

## Work items

### Core runtime

- [ ] 04.1 — `WorkItemCell<F>`: `static`-allocated cell owning a pinned
      future and its `RawWaker`. Accepts an `F: Future + 'static`.
- [ ] 04.2 — `RawWakerVTable` whose `wake` / `wake_by_ref` call
      `px4_sys::WorkItem_ScheduleNow(work_item_ptr)`. `clone` is a no-op
      (waker identity == work-item pointer).
- [ ] 04.3 — `ScheduledWorkItem` C++ trampoline in `px4-sys`: its `Run()`
      calls `extern "C" fn rust_work_item_poll(cell_ptr)` which poll-calls
      the future.
- [ ] 04.4 — `wq_configurations` Rust enum generated from
      `platforms/common/px4_work_queue/WorkQueueManager.hpp`

### Primitives

- [ ] 04.5 — `AtomicWaker`: lock-free single-slot `Waker` store. Port
      `futures::task::AtomicWaker` (no alloc).
- [ ] 04.6 — `Timer`: wraps `hrt_call_every`; `tick().await` registers
      the calling task's waker and returns on the next callback.
- [ ] 04.7 — `Notify`: cross-task signal. Register + wake pattern.
- [ ] 04.8 — `Channel<T, const N: usize>`: heapless SPSC with waker
      notification on both sides.

### `#[task]` macro

- [ ] 04.9 — `crates/px4-workqueue-macros/` with proc-macro
      `#[task(wq = "...")]`
- [ ] 04.10 — Expansion: generate `mod <fn_name> { static CELL; pub fn spawn(args); }`
- [ ] 04.11 — Validation: `wq` argument must match a `wq_configurations` variant
      (compile error otherwise)
- [ ] 04.12 — `trybuild` tests for good + bad invocations

## Acceptance criteria

- [ ] A `#[task(wq = "test")] async fn foo(x: u32) { ... }` compiles
- [ ] Calling `foo::spawn(42)` twice panics (single-shot spawn)
- [ ] Host-side unit test: mock `px4-sys` `ScheduleNow` as a channel send,
      drive the runtime in a loop, verify future completes
- [ ] No heap allocation in the `spawn` path (verify with `--cfg forbid_alloc`)
- [ ] Compiles on `thumbv7em-none-eabihf` against real `px4-sys`

## Open questions

- Should `spawn()` be `unsafe`? It mutates a `static mut` (via `UnsafeCell`).
  Lean toward safe since the cell's `init` uses atomic CAS to detect double-init.
- How to express "spawn this task on whichever WQ my caller is on"? Probably
  a separate `#[task_on_caller]` attribute — deferred.
