use core::cell::UnsafeCell;
use core::marker::PhantomData;

use heapless::spsc::{Consumer, Producer, Queue};

/// A lock-free single-producer/single-consumer queue for passing events between threads.
pub struct ClapEventQueue<T, const N: usize> {
    inner: UnsafeCell<Queue<T, N>>,
}

unsafe impl<T: Send, const N: usize> Send for ClapEventQueue<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for ClapEventQueue<T, N> {}

impl<T: Copy, const N: usize> ClapEventQueue<T, N> {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Queue::new()),
        }
    }

    pub fn split(&self) -> (EventWriter<'_, T, N>, EventSlice<'_, T, N>) {
        // Safety: Queue provides interior mutability via split.
        let queue = unsafe { &mut *self.inner.get() };
        let (producer, consumer) = queue.split();
        (
            EventWriter {
                producer,
                _marker: PhantomData,
            },
            EventSlice {
                consumer,
                _marker: PhantomData,
            },
        )
    }
}

pub struct EventWriter<'a, T, const N: usize> {
    producer: Producer<'a, T, N>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy, const N: usize> EventWriter<'a, T, N> {
    pub fn push(&mut self, event: T) -> Result<(), T> {
        self.producer.enqueue(event)
    }
}

pub struct EventSlice<'a, T, const N: usize> {
    consumer: Consumer<'a, T, N>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy, const N: usize> EventSlice<'a, T, N> {
    pub fn pop(&mut self) -> Option<T> {
        self.consumer.dequeue()
    }
}
