# Phase 04 — `px4-workqueue` + `#[task]` macro

**Goal**: Rust async runtime on PX4 WorkQueue, 1 task ≡ 1 WorkItem. This
is the project's signature crate.

**Status**: Core landed. Primitives (Timer/Notify/Channel) deferred to a follow-up.
**Priority**: P0
**Depends on**: Phase 02, Phase 03

## Architecture

See [docs/async-model.md](../async-model.md) and
[docs/task-macro.md](../task-macro.md).

## Work items

### Core runtime

- [x] 04.1 — `WorkItemCell<F>` — static cell with `#[repr(C)]` prefix
      `TaskStateBits { state: AtomicU8, handle: AtomicPtr<WorkItem> }`
      plus `UnsafeCell<MaybeUninit<F>>`. Generic over `F: Future<Output = ()> + Send`.
- [x] 04.2 — Universal `RawWakerVTable` in `waker.rs`. Waker data pointer
      is `&TaskStateBits`; `wake_by_ref` does `fetch_or(RUN_QUEUED)` and
      only calls `px4_rs_wi_schedule_now(handle)` on a `SPAWNED & !RUN_QUEUED`
      transition. `clone` just copies the pointer; `drop` is a no-op.
- [x] 04.3 — Rust-side `run_trampoline` is monomorphized per F and
      registered with `px4_rs_wi_new` via its ctx+run_fn pair. No change
      to `px4-sys/wrapper.cpp` required.
- [x] 04.4 — `wq_configurations` constants in `wq.rs` (hand-transcribed
      from PX4 v1.16.2; identical to v1.15 and v1.17-rc2).

### Primitives

- [x] 04.5 — `AtomicWaker` ported from `futures-util` (no alloc).
- [x] 04.6 — `sleep(Duration)` — pinned `Future` that arms PX4's
      `hrt_call_after` on first poll, wakes its waker from the HRT
      callback, and runs `hrt_cancel` on Drop. Host mock fans out a
      short-lived std thread per timer; cancellation is a flag the
      thread checks before firing.
- [x] 04.7 — `Notify` — single-waiter edge-triggered signal modeled
      on `tokio::sync::Notify::notify_one`. Stores at most one
      permit; multiple notifies coalesce.
- [ ] 04.8 — `Channel<T, const N: usize>` — deferred.

### `#[task]` macro

- [x] 04.9 — `crates/px4-workqueue-macros/` with proc-macro
      `#[task(wq = "...")]`
- [x] 04.10 — Expansion: generates a module named after the function
      containing `type __Fut = impl Future<Output = ()>` (TAIT),
      `static __CELL: WorkItemCell<__Fut>`, `fn __make(args) -> __Fut`
      (with `#[define_opaque(__Fut)]`), and public `spawn` / `try_spawn`.
      Users must enable `#![feature(type_alias_impl_trait)]`.
- [x] 04.11 — `wq` validation: the expansion references
      `wq_configurations::<name>` by identifier, so a typo is a
      compile-time "no such constant" error with span at the `"..."` literal.
- [ ] 04.12 — `trybuild` tests for good + bad invocations — deferred
      (positive-path covered by `tests/task_macro.rs`).

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

- [x] A `#[task(wq = "test1")] async fn foo(x: u32) { ... }` compiles
- [x] `foo::try_spawn(42)` returns `Ok(SpawnToken)` on first call,
      `Err(SpawnError::Busy)` on a second call while the task is still
      running (covered by `tests/basic.rs::double_spawn_returns_busy`)
- [x] After the task's future resolves, a subsequent `spawn(42)`
      succeeds (covered by `tests/basic.rs::respawn_after_finish`)
- [x] A `SpawnToken` that is dropped without being `.forget()`-ed panics
      (enforced by `impl Drop for SpawnToken`)
- [x] Host-side unit test: mock `px4-sys` `ScheduleNow` as an mpsc send
      in `src/ffi.rs::mock`, drive the runtime, verify future completes
      (`tests/basic.rs`, `tests/task_macro.rs`)
- [x] No heap allocation in the `spawn` path — the non-`std` build path
      calls only `px4_rs_wi_new` / `px4_rs_wi_schedule_now` and writes
      into the static cell's `MaybeUninit` slot; no `Box`, no `alloc`
- [x] Compiles on `thumbv7em-none-eabihf` against real `px4-sys`
