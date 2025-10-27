pub mod rt_event;

use parking_lot::Mutex;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::OnceLock;

use self::rt_event::RtEvent;

struct RtRingStore {
    producer: Mutex<Option<Producer<RtEvent>>>,
    consumer: Mutex<Option<Consumer<RtEvent>>>,
}

static RT_RING: OnceLock<RtRingStore> = OnceLock::new();

pub fn init_rt_ring(capacity: usize) -> (Producer<RtEvent>, Consumer<RtEvent>) {
    let (prod, cons) = RingBuffer::<RtEvent>::new(capacity);
    let _ = RT_RING.set(RtRingStore {
        producer: Mutex::new(Some(prod)),
        consumer: Mutex::new(Some(cons)),
    });
    let ring = RT_RING.get().expect("rt ring not inited");
    let producer = ring
        .producer
        .lock()
        .take()
        .expect("rt producer already taken");
    let consumer = ring
        .consumer
        .lock()
        .take()
        .expect("rt consumer already taken");
    (producer, consumer)
}

pub fn take_ui_consumer() -> Consumer<RtEvent> {
    RT_RING
        .get()
        .expect("rt ring not inited")
        .consumer
        .lock()
        .take()
        .expect("rt consumer already taken")
}
