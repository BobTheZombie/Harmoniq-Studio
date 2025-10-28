/// Utility for mapping monotonic timestamps into audio sample offsets.
#[derive(Debug, Clone)]
pub struct MidiClock {
    sample_rate: u32,
    last_nanos: u64,
    last_sample_pos: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_nanos_to_block_offset() {
        let mut clock = MidiClock::new(48_000);
        clock.advance_transport(0, 1000);
        let offset = clock.to_block_sample(1_000_000, 1000, 128);
        assert_eq!(offset, 48); // 1ms at 48k = 48 samples

        let late = clock.to_block_sample(10_000_000_000, 1000, 128);
        assert_eq!(late, 127); // clamped to end of block
    }
}

impl MidiClock {
    /// Create a new clock for the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            last_nanos: 0,
            last_sample_pos: 0,
        }
    }

    /// Update the clock with transport progress information.
    pub fn advance_transport(&mut self, nanos: u64, sample_pos: u64) {
        self.last_nanos = nanos;
        self.last_sample_pos = sample_pos;
    }

    /// Translate a monotonic timestamp into a sample offset within an audio block.
    pub fn to_block_sample(
        &mut self,
        now_nanos: u64,
        block_start_sample: u64,
        block_frames: u32,
    ) -> u32 {
        if block_frames == 0 {
            return 0;
        }
        let nanos_per_sample = 1_000_000_000u64 / self.sample_rate.max(1) as u64;
        let expected_sample = if now_nanos >= self.last_nanos {
            self.last_sample_pos + (now_nanos - self.last_nanos) / nanos_per_sample
        } else {
            block_start_sample
        };
        let mut offset = expected_sample.saturating_sub(block_start_sample) as i64;
        if offset < 0 {
            offset = 0;
        }
        if offset as u32 >= block_frames {
            block_frames - 1
        } else {
            offset as u32
        }
    }
}
