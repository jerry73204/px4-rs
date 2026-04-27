//! Host-mock tests for the SPSC `Channel`.

#![cfg(feature = "std")]
#![feature(type_alias_impl_trait)]

use core::future::Future;
use core::sync::atomic::{AtomicU32, Ordering};

use px4_workqueue::{Channel, WorkItemCell, drain_until_idle, wq_configurations};

mod try_send_recv_basic {
    use super::*;

    #[test]
    fn empty_full_cycle() {
        let ch: Channel<u32, 4> = Channel::new();
        assert!(ch.is_empty());
        assert!(!ch.is_full());

        ch.try_send(1).expect("first send");
        ch.try_send(2).expect("second send");
        ch.try_send(3).expect("third send");
        ch.try_send(4).expect("fourth send");
        assert!(ch.is_full());
        assert_eq!(ch.try_send(5), Err(5), "fifth send should reject");

        assert_eq!(ch.try_recv(), Some(1));
        assert_eq!(ch.try_recv(), Some(2));
        assert_eq!(ch.try_recv(), Some(3));
        assert_eq!(ch.try_recv(), Some(4));
        assert!(ch.is_empty());
        assert_eq!(ch.try_recv(), None);
    }
}

mod async_pipe_through_runtime {
    use super::*;

    static CH: Channel<u32, 8> = Channel::new();
    static SUM: AtomicU32 = AtomicU32::new(0);
    static SENT: AtomicU32 = AtomicU32::new(0);

    type ProdFut = impl Future<Output = ()>;
    static PROD: WorkItemCell<ProdFut> = WorkItemCell::new();

    type ConsFut = impl Future<Output = ()>;
    static CONS: WorkItemCell<ConsFut> = WorkItemCell::new();

    #[define_opaque(ProdFut)]
    fn make_producer() -> ProdFut {
        async {
            for i in 1..=10 {
                CH.send(i).await;
                SENT.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    #[define_opaque(ConsFut)]
    fn make_consumer() -> ConsFut {
        async {
            for _ in 0..10 {
                let v = CH.recv().await;
                SUM.fetch_add(v, Ordering::AcqRel);
            }
        }
    }

    #[test]
    fn test() {
        CONS.spawn(make_consumer(), &wq_configurations::test1, c"chan_cons")
            .forget();
        PROD.spawn(make_producer(), &wq_configurations::test1, c"chan_prod")
            .forget();

        for _ in 0..50 {
            if SUM.load(Ordering::Acquire) == 55 {
                break;
            }
            drain_until_idle();
        }

        assert_eq!(SENT.load(Ordering::Acquire), 10, "all 10 sends ran");
        assert_eq!(
            SUM.load(Ordering::Acquire),
            55,
            "consumer should have summed 1+...+10 = 55"
        );
    }
}

mod backpressure_blocks_send {
    use super::*;

    static CH: Channel<u32, 2> = Channel::new();
    static PROD_DONE: AtomicU32 = AtomicU32::new(0);

    type ProdFut = impl Future<Output = ()>;
    static PROD: WorkItemCell<ProdFut> = WorkItemCell::new();

    #[define_opaque(ProdFut)]
    fn make_producer() -> ProdFut {
        async {
            // Capacity 2 — first two sends fill, third must park.
            CH.send(10).await;
            CH.send(20).await;
            CH.send(30).await;
            PROD_DONE.store(1, Ordering::Release);
        }
    }

    #[test]
    fn test() {
        PROD.spawn(make_producer(), &wq_configurations::test1, c"chan_bp")
            .forget();

        drain_until_idle();
        assert_eq!(
            PROD_DONE.load(Ordering::Acquire),
            0,
            "producer should still be parked on the third send (channel full)"
        );
        assert_eq!(CH.len(), 2, "buffer holds the first two");

        // Drain one slot — that wakes the producer.
        assert_eq!(CH.try_recv(), Some(10));
        drain_until_idle();
        assert_eq!(
            PROD_DONE.load(Ordering::Acquire),
            1,
            "producer should resume after recv freed a slot"
        );

        assert_eq!(CH.try_recv(), Some(20));
        assert_eq!(CH.try_recv(), Some(30));
    }
}
