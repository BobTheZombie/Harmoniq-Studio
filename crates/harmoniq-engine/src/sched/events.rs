use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::vec::Vec;

#[derive(Clone, Debug)]
pub enum Ev {
    Midi([u8; 3], u32),
    Param { id: u32, norm: f32, sample: u32 },
}

impl Default for Ev {
    fn default() -> Self {
        Ev::Midi([0; 3], 0)
    }
}

impl Ev {
    fn sample_u64(&self) -> u64 {
        match *self {
            Ev::Midi(_, sample) => sample as u64,
            Ev::Param { sample, .. } => sample as u64,
        }
    }

    fn set_sample(&mut self, sample: u32) {
        match self {
            Ev::Midi(_, s) => *s = sample,
            Ev::Param { sample: s, .. } => *s = sample,
        }
    }
}

pub struct EventLane {
    buf: Vec<Ev>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    scratch: UnsafeCell<Vec<Ev>>,
}

unsafe impl Send for EventLane {}
unsafe impl Sync for EventLane {}

impl EventLane {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "event lane capacity must be non-zero");
        let mut scratch = Vec::with_capacity(capacity);
        scratch.clear();
        Self {
            buf: vec![Ev::default(); capacity],
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            scratch: UnsafeCell::new(scratch),
        }
    }

    fn advance(&self, index: usize) -> usize {
        let next = index + 1;
        if next >= self.capacity {
            0
        } else {
            next
        }
    }

    pub fn push(&self, ev: Ev) -> Result<(), Ev> {
        let head = self.head.load(Ordering::Relaxed);
        let next = self.advance(head);
        if next == self.tail.load(Ordering::Acquire) {
            return Err(ev);
        }
        unsafe {
            *self.buf.get_unchecked_mut(head) = ev;
        }
        self.head.store(next, Ordering::Release);
        Ok(())
    }
}

pub struct EventSlice<'a> {
    pub ev: &'a [Ev],
}

impl<'a> EventSlice<'a> {
    pub const EMPTY: Self = EventSlice { ev: &[] };
}

pub fn slice_for_block<'a>(lane: &'a EventLane, start: u64, frames: u32) -> EventSlice<'a> {
    if frames == 0 {
        return EventSlice::EMPTY;
    }

    let end = start + frames as u64;
    let head = lane.head.load(Ordering::Acquire);
    let mut tail = lane.tail.load(Ordering::Acquire);
    let mut new_tail = tail;
    let scratch = unsafe { &mut *lane.scratch.get() };
    scratch.clear();

    while tail != head {
        let ev = unsafe { lane.buf.get_unchecked(tail).clone() };
        let sample = ev.sample_u64();
        if sample < start {
            new_tail = lane.advance(tail);
            tail = new_tail;
            continue;
        }
        if sample >= end {
            break;
        }

        let mut adjusted = ev;
        adjusted.set_sample((sample - start) as u32);
        scratch.push(adjusted);

        new_tail = lane.advance(tail);
        tail = new_tail;
    }

    lane.tail.store(new_tail, Ordering::Release);

    EventSlice {
        ev: scratch.as_slice(),
    }
}
