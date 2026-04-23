# Async model

## One task ≡ one WorkItem

A `#[task(wq = "...")]` function compiles to a `ScheduledWorkItem` subclass.
Its `Run()` method polls the future exactly once. If the future returns
`Pending`, `Run()` returns and the WorkItem sits idle on the WorkQueue's
attached list until something calls `ScheduleNow()` on it.

There is no per-executor ready queue inside Rust. PX4's `WorkQueue::_q`
is the only ready queue.

## Wake chain

On any event that should resume a suspended task:

```
event source (uORB publish, hrt timer, cross-task Notify, …)
        │
        ▼
  callback body calls AtomicWaker::wake()
        │
        ▼
  RawWaker vtable → px4_schedule_now(work_item_ptr)
        │
        ▼
  WorkQueue::Add(item): _q.push(item); sem_post(_process_lock)
        │
        ▼
  WorkQueue pthread wakes from sem_wait, drains _q
        │
        ▼
  item->Run() — polls the future
```

The future's `poll` runs the user code up to its next `.await`, registers
its `Waker` in the wake slot of whichever source it's waiting on, and
returns `Pending`. End of the cycle. Nothing runs until the next event.

## Wake slots

Every suspendable primitive owns a single `AtomicWaker`:

| Primitive | Slot owner |
| --------- | ---------- |
| `Subscription<M>::recv()` | the `Subscription` itself |
| `Timer::tick()` | the `Timer` |
| `Notify::notified()` | the `Notify` |
| `Channel<T>::recv()` | receiver-side |

`AtomicWaker` holds one `Waker`; a second `register` replaces the first. This
matches the "one waiting task per primitive" contract — enforced by the API,
since these are `&mut self` methods.

## Multi-source tasks

A single task can wait on multiple sources with `futures::select_biased!` /
`join!` — each sub-future registers in its own slot, and any wake-up
causes the task to be re-polled. All sub-futures run their `poll` again;
the ones not ready return `Pending` and re-register. This is the standard
`Future` trait contract — no special machinery.

Alternative and often cleaner on PX4: split into two `#[task]`s on the
same or different WQs. Two WorkItems, independent wake paths, zero
cross-future re-polling.

## Cold-start semantics

`spawn()` calls `WorkQueueManager::Attach(item, wq_config)` and then
`ScheduleNow()` exactly once so the first poll happens. After that, the
task only runs in response to events.

## Dropping tasks

Dropping a `WorkItemCell` (the static container for a task's future)
calls `Detach` and, if it was the last item on that WQ, triggers the
WQ's orderly shutdown. In practice most PX4 modules never drop — they
run for the life of the firmware.

## Why this isn't Embassy

Embassy's executor owns an intrusive ready-queue inside the Rust runtime.
On PX4 that queue would be a redundant second copy of `WorkQueue::_q`.
px4-rs deletes the Rust-side queue and wires the Waker straight to the
PX4 primitive.

The cost is one indirect call through the `RawWakerVTable` on every wake,
plus ~24 bytes of per-task overhead for the WorkItem base class. Compared
to Embassy: strictly less overhead, at the price of giving up
`spawner.spawn(fut)` of arbitrary futures (you must define tasks at
compile time via `#[task]`, same as Embassy's static-executor mode).
