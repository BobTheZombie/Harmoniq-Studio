/// Utility helpers for converting between musical and time domains.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo(pub f32);

impl Tempo {
    pub fn beats_per_second(&self) -> f32 {
        self.0 / 60.0
    }

    pub fn seconds_per_beat(&self) -> f32 {
        1.0 / self.beats_per_second()
    }

    pub fn samples_per_beat(&self, sample_rate: f32) -> f32 {
        self.seconds_per_beat() * sample_rate
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub fn four_four() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}
