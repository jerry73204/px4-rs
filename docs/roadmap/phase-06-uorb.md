# Phase 06 — `px4-uorb` typed pub/sub

**Goal**: Safe typed `Publication<M>` / `Subscription<M>` with async
`recv()` driven by uORB's `SubscriptionCallback`.

**Status**: Complete (single-instance; advertise_multi/update_rate_hz deferred)
**Priority**: P0
**Depends on**: Phase 02, Phase 04, Phase 05

## API sketch

```rust
pub trait UorbTopic: Sized + 'static {
    fn metadata() -> &'static orb_metadata;
}

pub struct Publication<M: UorbTopic> { /* orb_advert_t + PhantomData */ }
impl<M: UorbTopic> Publication<M> {
    pub fn advertise() -> Self;
    pub fn advertise_multi(instance: &mut i32, priority: OrbPriority) -> Self;
    pub fn publish(&self, msg: &M) -> Result<(), PubError>;
}

pub struct Subscription<M: UorbTopic> { /* orb_sub + AtomicWaker */ }
impl<M: UorbTopic> Subscription<M> {
    pub fn subscribe() -> Self;
    pub fn recv(&mut self) -> impl Future<Output = M> + '_;  // async
    pub fn try_recv(&mut self) -> Option<M>;
    pub fn update_rate_hz(&self, hz: u32) -> Result<(), SubError>;
}
```

## Work items

- [x] 06.1 — `Publication<T>` over `orb_advertise_multi` + `orb_publish`.
      Lazy advertise on first publish; CAS handles concurrent first-use.
      Send + Sync (PX4 documents `orb_advert_t` as globally shareable).
- [x] 06.2 — `Subscription<T>` over the `px4_rs_sub_cb_*` trampolines.
      `!Send` to anchor `&self.waker` to the pinned future state and
      sidestep "stable callback ctx pointer" gymnastics.
- [x] 06.3 — `SubscriptionCallback` C++ trampoline already shipped in
      phase 02 (`px4_rs_sub_cb_new` / `register` / `update`).
- [x] 06.4 — `recv()` future: try_recv first; on miss, register waker
      and re-check before returning Pending.
- [x] 06.5 — Overrun semantics documented in `Subscription` rustdoc;
      the integration test uses an explicit `yield_now().await` between
      publishes so the subscriber consumes each sample. `recv_all` is
      deferred — implementable as a thin loop on `try_recv`.
- [ ] 06.6 — `update_rate_hz`, `advertise_multi(instance, priority)`,
      and `queue_size` knobs — deferred. The trampoline's `interval_us`
      and `instance` parameters are already plumbed; surfacing them is
      a small follow-up.
- [x] 06.7 — Integration test (`tests/round_trip.rs`): publisher and
      subscriber `#[task]`s on the same WQ, 1000 samples round-trip,
      verifies count and last-sample contents.

## Acceptance criteria

- [x] Round-trip test: publisher publishes 1000 samples; subscriber
      receives exactly 1000 (with explicit `yield_now()` between
      publishes — see "overrun semantics" note above; broker-level
      queueing is a real-PX4 feature that the host mock does not
      replicate)
- [x] `recv().await` returns within one WQ cycle of a publish (the
      `SubscriptionCallback` trampoline calls our wake fn synchronously
      from inside `orb_publish`)
- [x] Safety: subscribing after the last publisher unadvertises returns
      `Pending` forever (the broker entry survives via subscriber Arc;
      `try_recv` returns false, future re-registers waker)

## Resolved open question

`Subscription<T>` is **`!Send`**. Rationale matches the design note:
the `SubscriptionCallback` C++ object stores `ctx = &self.waker`, so
the Subscription value cannot move after the first `recv()` poll.
Marking it `!Send` (via `PhantomData<*const ()>`) plus `PhantomPinned`
encodes that constraint in the type system. The natural usage —
holding the Subscription as a local in an `async fn`, which becomes a
field of the pinned future state in its `WorkItemCell` — satisfies it.

## Limitations

- Synthesized `orb_metadata` uses `message_hash = 0` and
  `o_id = u16::MAX`. Cross-language interop with PX4 C++ publishers
  works by `o_name` matching, but PX4 code paths that strictly check
  `message_hash` for compatibility will reject our publications. A
  follow-up phase can replicate PX4's hash function or link against
  `__orb_<name>` symbols directly.
