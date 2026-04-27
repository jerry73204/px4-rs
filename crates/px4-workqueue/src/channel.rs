//! `Channel<T, const N>` — bounded SPSC async channel.
//!
//! Single-producer, single-consumer, fixed-capacity. The buffer is
//! `N` slots of `T` with no heap allocation; concurrent access is
//! disciplined by two unsigned counters (`head` for reads, `tail`
//! for writes) on a wrapping arithmetic.
//!
//! # SPSC contract
//!
//! At most one task may call `send` at a time, and at most one task
//! may call `recv` at a time. Two senders or two receivers will race
//! on the buffer slot. The runtime guarantees one pollable task per
//! `WorkItemCell`, so as long as a producer task and a consumer task
//! own each half, this contract is upheld by construction.
//!
//! For a multi-producer fan-in, give each producer its own channel
//! and `select!` on the receiving end. A future MPMC primitive can
//! be added later if a real call site needs it.
//!
//! ```ignore
//! use px4_workqueue::{Channel, task};
//!
//! static CH: Channel<u32, 16> = Channel::new();
//!
//! #[task(wq = "lp_default")]
//! async fn producer() {
//!     for i in 0.. {
//!         CH.send(i).await;
//!     }
//! }
//!
//! #[task(wq = "lp_default")]
//! async fn consumer() {
//!     loop {
//!         let v = CH.recv().await;
//!         px4_log::info!("got {v}");
//!     }
//! }
//! ```

use core::cell::UnsafeCell;
use core::future::Future;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::{Context, Poll};

use crate::AtomicWaker;

/// Bounded SPSC channel of `T` with `N` slots.
pub struct Channel<T, const N: usize> {
    buf: [UnsafeCell<MaybeUninit<T>>; N],
    /// Read counter — incremented by `recv`. Wrapping arithmetic on
    /// `tail - head` gives the live count, modulo `usize` overflow.
    head: AtomicUsize,
    /// Write counter — incremented by `send`.
    tail: AtomicUsize,
    /// Wakes the consumer when a sample lands.
    recv_waker: AtomicWaker,
    /// Wakes the producer when a slot frees up.
    send_waker: AtomicWaker,
}

// SAFETY: cross-task access is fully disciplined by `head`/`tail`.
// The producer touches `buf[tail % N]` while `tail - head < N`;
// the consumer touches `buf[head % N]` while `head < tail`. The
// SPSC contract (one sender, one receiver) plus Acquire/Release on
// the counters keeps these regions disjoint. (Fully-qualifying the
// marker traits here so the local `Send` future type doesn't shadow
// `core::marker::Send`.)
unsafe impl<T: core::marker::Send, const N: usize> core::marker::Send for Channel<T, N> {}
unsafe impl<T: core::marker::Send, const N: usize> core::marker::Sync for Channel<T, N> {}

impl<T, const N: usize> Channel<T, N> {
    /// Construct an empty channel.
    pub const fn new() -> Self {
        // Build the array element-by-element since `[UnsafeCell<...>; N]`
        // can't be initialised with `[init; N]` in a const fn.
        Self {
            buf: [const { UnsafeCell::new(MaybeUninit::uninit()) }; N],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            recv_waker: AtomicWaker::new(),
            send_waker: AtomicWaker::new(),
        }
    }

    /// Channel capacity (= `N`).
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Number of values currently buffered. The result is a snapshot;
    /// concurrent send/recv may change it before the caller acts.
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// True if the channel currently holds no values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True if the channel currently holds `N` values.
    pub fn is_full(&self) -> bool {
        self.len() >= N
    }

    /// Non-blocking send. Returns `Err(v)` if the channel is full.
    pub fn try_send(&self, v: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(head) >= N {
            return Err(v);
        }
        let idx = tail % N;
        // SAFETY: SPSC contract — only this producer writes to the
        // buf slot at `tail`. The consumer reads from `head`, which
        // we've already observed as `tail.wrapping_sub(head) < N`,
        // i.e. head != tail's slot.
        unsafe {
            (*self.buf[idx].get()).write(v);
        }
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        self.recv_waker.wake();
        Ok(())
    }

    /// Non-blocking recv. Returns `None` if the channel is empty.
    pub fn try_recv(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Relaxed);
        if head == tail {
            return None;
        }
        let idx = head % N;
        // SAFETY: head < tail (as observed under Acquire), so the slot
        // was written by a Release-paired producer. SPSC guarantees
        // we're the sole reader.
        let v = unsafe { (*self.buf[idx].get()).assume_init_read() };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        self.send_waker.wake();
        Some(v)
    }

    /// Build a future that resolves once the value has been pushed.
    pub fn send(&self, v: T) -> Send<'_, T, N> {
        Send {
            chan: self,
            value: Some(v),
        }
    }

    /// Build a future that resolves with the next received value.
    pub fn recv(&self) -> Recv<'_, T, N> {
        Recv { chan: self }
    }
}

impl<T, const N: usize> Default for Channel<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Drop for Channel<T, N> {
    fn drop(&mut self) {
        // Drain any leftover values so their `Drop` runs. SPSC
        // promise is suspended here — `&mut self` means there's no
        // contender — so we can use plain index math.
        while let Some(v) = self.try_recv() {
            drop(v);
        }
    }
}

/// Future returned by [`Channel::send`].
#[must_use = "futures do nothing unless awaited"]
pub struct Send<'a, T, const N: usize> {
    chan: &'a Channel<T, N>,
    /// Optional so we can `.take()` it on the success path. `None`
    /// after Ready.
    value: Option<T>,
}

impl<'a, T, const N: usize> Future for Send<'a, T, N> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: `Send` holds nothing self-referential — `value` is a
        // plain `Option<T>` and `chan` is a 'static reference. Moving
        // through `get_unchecked_mut` is sound; we never construct a
        // `Pin` that promises otherwise.
        let this = unsafe { self.get_unchecked_mut() };
        let value = this.value.take().expect("Send polled after Ready");
        match this.chan.try_send(value) {
            Ok(()) => Poll::Ready(()),
            Err(v) => {
                // Park before the second try so we don't miss a wake
                // that lands between the first try and our register.
                this.chan.send_waker.register(cx.waker());
                match this.chan.try_send(v) {
                    Ok(()) => Poll::Ready(()),
                    Err(v) => {
                        this.value = Some(v);
                        Poll::Pending
                    }
                }
            }
        }
    }
}

/// Future returned by [`Channel::recv`].
#[must_use = "futures do nothing unless awaited"]
pub struct Recv<'a, T, const N: usize> {
    chan: &'a Channel<T, N>,
}

impl<'a, T, const N: usize> Future for Recv<'a, T, N> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        if let Some(v) = self.chan.try_recv() {
            return Poll::Ready(v);
        }
        self.chan.recv_waker.register(cx.waker());
        if let Some(v) = self.chan.try_recv() {
            return Poll::Ready(v);
        }
        Poll::Pending
    }
}
