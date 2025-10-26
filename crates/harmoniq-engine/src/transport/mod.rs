use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[derive(Debug)]
pub struct Transport {
    pub playing: AtomicBool,
    pub sample_pos: AtomicU64,
    pub sr: AtomicU32,
}

impl Transport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sample_rate(sr: u32) -> Self {
        Self {
            playing: AtomicBool::new(false),
            sample_pos: AtomicU64::new(0),
            sr: AtomicU32::new(sr),
        }
    }

    pub fn pos(&self) -> u64 {
        self.sample_pos.load(Ordering::Relaxed)
    }

    pub fn seconds(&self) -> f64 {
        let sp = self.sample_pos.load(Ordering::Relaxed) as f64;
        let sr = self.sr.load(Ordering::Relaxed).max(1) as f64;
        sp / sr
    }

    pub fn set_sample_rate(&self, sr: u32) {
        self.sr.store(sr, Ordering::Relaxed);
    }

    pub fn sample_rate(&self) -> u32 {
        self.sr.load(Ordering::Relaxed)
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::with_sample_rate(48_000)
    }
}

impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            playing: AtomicBool::new(self.playing.load(Ordering::Relaxed)),
            sample_pos: AtomicU64::new(self.sample_pos.load(Ordering::Relaxed)),
            sr: AtomicU32::new(self.sr.load(Ordering::Relaxed)),
        }
    }
}
