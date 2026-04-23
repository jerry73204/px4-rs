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
- [ ] 04.10 — Expansion: generate
      `mod <fn_name> { static CELL; pub fn spawn(args) -> Result<SpawnToken, SpawnError>; }`
- [ ] 04.11 — Validation: `wq` argument must match a `wq_configurations` variant
      (compile error otherwise)
- [ ] 04.12 — `trybuild` tests for good + bad invocations

## Spawn API shape (decided)

Follow Embassy's `TaskStorage::spawn`:

```rust
pub fn spawn(args...) -> Result<SpawnToken, SpawnError>;
```

- **Safe, fallible.** The per-task `static CELL` uses an `AtomicU8` state
  word; `spawn` does a `compare_exchange(IDLE, SPAWNED, AcqRel, Acquire)`
  and returns `Err(SpawnError::Busy)` on failure. An `unsafe fn spawn`
  would push a contract onto every caller that the CAS already enforces
  for free — ecosystem precedent (Embassy, RTIC, `static_cell`) is
  unanimous here.
- **`SpawnToken` is `#[must_use]` and its `Drop` panics**, so "forgot to
  hand it to the executor" is caught at runtime. Same trick Embassy uses.
- **Clear the init flag *last* in the post-`Poll::Ready` path.** This
  makes respawn-after-finish legal: after a task's future drops, the
  slot returns to `IDLE` and a subsequent `spawn` succeeds. Useful for
  supervisors, watchdog-driven restarts, and long-lived modules that
  restart sub-logic on error.
- **`try_spawn` / `spawn` pair** — the generated module exposes both:
  `spawn` panics on `Busy` (ergonomic one-liner for cold-start),
  `try_spawn` returns `Result` (for supervisors).

## Spawn-on-caller's-WQ (deferred, design recorded)

Out of scope for phase 04. The trampoline must be shaped to support it
later, though:

- The C++ `ScheduledWorkItem::Run()` trampoline in `px4-sys/wrapper.cpp`
  shall write `CURRENT_WQ: *const WqConfig` into a pthread-local slot
  **before** calling the Rust `poll` function, and clear it after. Each
  PX4 WorkQueue is its own pthread, so the slot is per-WQ-thread with
  zero contention.
- A follow-up phase adds `px4_workqueue::current_wq() -> &'static WqConfig`
  (panics outside a WQ context) plus a `child::spawn_here(args)` sugar
  over `child::spawn_on(current_wq(), args)`.
- Rejected alternatives:
  - `#[task(wq = caller)]` — the child is a sibling item; at its
    *definition* site the caller is unknown. Would require generic tasks,
    which collides with the compile-time static-slot model.
  - Thread a `Spawner` handle through every async-fn argument — too
    much boilerplate for the ergonomics gain; this is why Embassy added
    `Spawner::for_current_executor()` to begin with.

## Acceptance criteria

- [ ] A `#[task(wq = "test")] async fn foo(x: u32) { ... }` compiles
- [ ] `foo::spawn(42)` returns `Ok(SpawnToken)` on first call,
      `Err(SpawnError::Busy)` on a second call while the task is still
      running
- [ ] After the task's future resolves, a subsequent `foo::spawn(42)`
      succeeds (respawn-after-finish)
- [ ] A `SpawnToken` that is dropped without being passed to the
      executor panics
- [ ] Host-side unit test: mock `px4-sys` `ScheduleNow` as a channel send,
      drive the runtime in a loop, verify future completes
- [ ] No heap allocation in the `spawn` path (verify with `--cfg forbid_alloc`)
- [ ] Compiles on `thumbv7em-none-eabihf` against real `px4-sys`
