//! Lock-free utilities for real-time safe communication.

use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::sync::Arc;

/// Real-time safe sender backed by a single-producer ring buffer.
pub struct RtSender<T> {
    producer: HeapProducer<T>,
}

impl<T> RtSender<T> {
    /// Attempts to push a message into the queue without blocking.
    #[inline(always)]
    pub fn try_send(&mut self, value: T) -> Result<(), T> {
        self.producer.push(value)
    }
}

/// Real-time safe receiver backed by a single-consumer ring buffer.
pub struct RtReceiver<T> {
    consumer: HeapConsumer<T>,
}

impl<T> RtReceiver<T> {
    /// Attempts to pop the next available message.
    #[inline(always)]
    pub fn try_recv(&mut self) -> Option<T> {
        self.consumer.pop()
    }
}

/// Creates a lock-free single-producer/single-consumer queue.
pub fn rt_queue<T>(capacity: usize) -> (RtSender<T>, RtReceiver<T>) {
    let rb = HeapRb::new(capacity);
    let (producer, consumer) = rb.split();
    (RtSender { producer }, RtReceiver { consumer })
}

/// Snapshot sender intended for non-realtime communication.
#[derive(Clone)]
pub struct SnapSender<T> {
    inner: Arc<parking_lot::Mutex<Vec<T>>>,
}

impl<T> SnapSender<T> {
    /// Pushes a new snapshot into the buffer, replacing older data.
    pub fn send(&self, value: T) {
        let mut guard = self.inner.lock();
        guard.clear();
        guard.push(value);
    }
}

/// Snapshot receiver matching [`SnapSender`].
pub struct SnapReceiver<T> {
    inner: Arc<parking_lot::Mutex<Vec<T>>>,
}

impl<T: Clone> SnapReceiver<T> {
    /// Attempts to retrieve the latest snapshot.
    pub fn recv_latest(&self) -> Option<T> {
        let guard = self.inner.lock();
        guard.last().cloned()
    }
}

/// Creates a snapshot channel pair.
pub fn snap_channel<T>() -> (SnapSender<T>, SnapReceiver<T>) {
    let inner = Arc::new(parking_lot::Mutex::new(Vec::with_capacity(1)));
    (
        SnapSender {
            inner: inner.clone(),
        },
        SnapReceiver { inner },
    )
}
