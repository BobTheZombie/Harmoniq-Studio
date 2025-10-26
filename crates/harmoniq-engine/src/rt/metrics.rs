use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, Default)]
pub struct BlockStat {
    pub ns: u64,
    pub frames: u32,
    pub xruns: u32,
}

pub struct Metrics {
    buf: Box<[UnsafeCell<BlockStat>]>,
    mask: usize,
    w: AtomicUsize,
    r: AtomicUsize,
}

unsafe impl Send for Metrics {}
unsafe impl Sync for Metrics {}

impl Metrics {
    pub fn new(cap: usize) -> Self {
        let capacity = cap.max(1).next_power_of_two().max(16);
        let mut buf = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buf.push(UnsafeCell::new(BlockStat::default()));
        }
        Self {
            buf: buf.into_boxed_slice(),
            mask: capacity - 1,
            w: AtomicUsize::new(0),
            r: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn write(&self, stat: BlockStat) {
        let write_idx = self.w.load(Ordering::Relaxed);
        let next = (write_idx.wrapping_add(1)) & self.mask;
        let read_idx = self.r.load(Ordering::Acquire);
        if next == read_idx {
            self.r
                .store((read_idx.wrapping_add(1)) & self.mask, Ordering::Release);
        }
        unsafe {
            *self.buf[write_idx & self.mask].get() = stat;
        }
        self.w.store(next, Ordering::Release);
    }

    pub fn read_all(&self) -> Vec<BlockStat> {
        let mut out = Vec::new();
        let mut read_idx = self.r.load(Ordering::Acquire);
        let write_idx = self.w.load(Ordering::Acquire);
        while read_idx != write_idx {
            let stat = unsafe { *self.buf[read_idx & self.mask].get() };
            out.push(stat);
            read_idx = (read_idx.wrapping_add(1)) & self.mask;
        }
        self.r.store(read_idx, Ordering::Release);
        out
    }
}
