use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Debug)]
pub struct MixerLevels {
    strips: Vec<StripLevel>,
}

#[derive(Debug)]
struct StripLevel {
    left_peak: AtomicU32,
    right_peak: AtomicU32,
    left_true: AtomicU32,
    right_true: AtomicU32,
    clipped: AtomicBool,
}

impl MixerLevels {
    pub fn new(count: usize) -> Self {
        Self {
            strips: (0..count).map(|_| StripLevel::new()).collect(),
        }
    }

    pub fn update(
        &self,
        idx: usize,
        left: f32,
        right: f32,
        left_true: f32,
        right_true: f32,
        clipped: bool,
    ) {
        if let Some(strip) = self.strips.get(idx) {
            strip.left_peak.store(left.to_bits(), Ordering::Relaxed);
            strip.right_peak.store(right.to_bits(), Ordering::Relaxed);
            strip
                .left_true
                .store(left_true.to_bits(), Ordering::Relaxed);
            strip
                .right_true
                .store(right_true.to_bits(), Ordering::Relaxed);
            strip.clipped.store(clipped, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self, idx: usize) -> (f32, f32, f32, f32, bool) {
        if let Some(strip) = self.strips.get(idx) {
            (
                f32::from_bits(strip.left_peak.load(Ordering::Relaxed)),
                f32::from_bits(strip.right_peak.load(Ordering::Relaxed)),
                f32::from_bits(strip.left_true.load(Ordering::Relaxed)),
                f32::from_bits(strip.right_true.load(Ordering::Relaxed)),
                strip.clipped.load(Ordering::Relaxed),
            )
        } else {
            (
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                false,
            )
        }
    }
}

impl StripLevel {
    fn new() -> Self {
        Self {
            left_peak: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            right_peak: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            left_true: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            right_true: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            clipped: AtomicBool::new(false),
        }
    }
}
