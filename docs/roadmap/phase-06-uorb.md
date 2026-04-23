# Phase 06 — `px4-uorb` typed pub/sub

**Goal**: Safe typed `Publication<M>` / `Subscription<M>` with async
`recv()` driven by uORB's `SubscriptionCallback`.

**Status**: Not Started
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

- [ ] 06.1 — `Publication<M>` over `orb_advertise_multi` + `orb_publish`
- [ ] 06.2 — `Subscription<M>` over `orb_subscribe` + `orb_copy` +
      `orb_register_callback`
- [ ] 06.3 — `SubscriptionCallback` C++ trampoline in `px4-sys` that
      stores a `AtomicWaker*` and calls `wake()` on update
- [ ] 06.4 — `recv()` future: try_recv first; if empty, register waker
      in the sub's `AtomicWaker`, return Pending
- [ ] 06.5 — Overrun semantics: document that `recv().await` returns only
      the latest sample between polls; add `recv_all(&mut buf)` for
      history if needed
- [ ] 06.6 — QoS knobs: `update_rate_hz`, `queue_size` on publication,
      matching C++ API
- [ ] 06.7 — Integration test: spawn two `#[task]`s, one publishes, one
      subscribes, verify N messages delivered

## Acceptance criteria

- [ ] Round-trip test: publisher task publishes 1000 samples at 1 kHz;
      subscriber task receives exactly 1000 (no drops under nominal load)
- [ ] `recv().await` returns within one WQ cycle of a publish
- [ ] Safety: double-subscribe to same topic in one task compiles and
      works; subscribing after the last publisher dropped returns
      `Pending` forever, not a hang + panic

## Open questions

- Should `Subscription<M>` be `Send`? uORB handles are per-thread in
  NuttX; making it `!Send` is safer but limits task migration. Lean
  toward `!Send` for the first cut.
