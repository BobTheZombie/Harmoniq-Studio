//! Lock-free SPSC/MPSC queues for control and automation data.

use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("queue is full")]
    Full,
    #[error("queue is empty")]
    Empty,
}

/// A bounded lock-free queue for communication between audio and control threads.
#[derive(Clone)]
pub struct EventQueue<T> {
    queue: Arc<ArrayQueue<T>>,
}

impl<T> EventQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    pub fn try_push(&self, value: T) -> Result<(), QueueError> {
        self.queue.push(value).map_err(|_| QueueError::Full)
    }

    pub fn try_pop(&self) -> Result<T, QueueError> {
        self.queue.pop().ok_or(QueueError::Empty)
    }

    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }
}
