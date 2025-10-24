use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug)]
pub struct Transport {
    pub playing: AtomicBool,
    pub sample_pos: AtomicU64,
    pub sample_rate: AtomicU64,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playing: AtomicBool::new(false),
            sample_pos: AtomicU64::new(0),
            sample_rate: AtomicU64::new(48_000),
        }
    }
}

impl Transport {
    #[inline]
    pub fn seconds(&self) -> f64 {
        let sp = self.sample_pos.load(Ordering::Relaxed);
        let sr = self.sample_rate.load(Ordering::Relaxed);
        if sr == 0 {
            return 0.0;
        }
        (sp as f64) / (sr as f64)
    }
}
