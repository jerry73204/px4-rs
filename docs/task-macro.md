# `#[task]` macro

## Attribute

```rust
#[task(wq = "rate_ctrl")]
async fn rate_watch(node: Node) -> ! { ... }
```

Mandatory argument:

- `wq` — string name of a `wq_configurations` entry. Looked up at
  compile time against a generated enum; mistyped names are a compile
  error.

Optional arguments (planned, not yet implemented):

- `pool = N` — spawn up to `N` instances of this task, each a distinct
  `WorkItem`. Default `pool = 1`.
- `stack_size = …` — only meaningful when `wq` is a newly-declared WQ;
  ignored for shared WQs.

## Expansion contract

Each `#[task]` generates a module with the same name as the function,
containing:

- `struct State` — typed wrapper for the future and its waker slot.
- `static CELL: WorkItemCell<State>` — `'static` storage. No heap.
- `pub fn spawn(args)` — builds the future, constructs the WorkItem,
  calls `WorkQueueManager::Attach`, and schedules the first poll.

The user code calls `rate_watch::spawn(...)` and never touches the
generated types directly.

## Static by default

Task futures live in `static` storage. This means:

1. Each `#[task]` has a fixed memory cost known at compile time.
2. `spawn` can be called at most once per task (double-spawn is a
   `panic!` or `Result::Err`, TBD).
3. Multiple concurrent instances of the same logic require `pool = N`
   at the attribute (planned).

This matches how Embassy, RTIC, and PX4's own C++ modules all behave.
No allocator required.

## Interaction with arguments

Arguments to the async fn are moved into the future at spawn time. They
must therefore be `Send + 'static`. The typical pattern:

```rust
#[task(wq = "rate_ctrl")]
async fn rate_watch(node: Node,
                    gyro_topic: &'static str) -> ! { ... }

rate_watch::spawn(node, "/fmu/out/sensor_gyro");
```

## Error handling

An async task returning `-> !` never finishes. An async task returning
`-> Result<(), Error>` that returns `Err` logs via `px4-log` and drops
the WorkItem. Returning `Ok(())` is treated the same way.

## Forbidden patterns

- `async move { ... }` blocks inside a `#[task]`-decorated function body
  are allowed, but nested `#[task]`s are not (macro expansion operates
  on the item, not the enclosing module).
- `tokio::spawn` / `futures::executor::block_on` — unavailable and would
  defeat the model if they were.
