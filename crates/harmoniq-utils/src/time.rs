//! Helpers for working with time and tempo conversions.

use crate::SampleTime;

/// Information describing timing context for the audio engine.
#[derive(Debug, Clone, Copy)]
pub struct TempoInfo {
    /// Tempo in beats per minute.
    pub bpm: f32,
    /// Number of beats per bar.
    pub time_signature_numerator: u32,
    /// Unit of the beat (4 = quarter notes).
    pub time_signature_denominator: u32,
}

impl Default for TempoInfo {
    fn default() -> Self {
        Self {
            bpm: 128.0,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
        }
    }
}

impl TempoInfo {
    /// Converts a duration expressed in beats to samples.
    #[inline]
    pub fn beats_to_samples(&self, beats: f32, sample_rate: f32) -> SampleTime {
        let seconds = beats * 60.0 / self.bpm.max(0.01);
        (seconds * sample_rate as f32) as SampleTime
    }

    /// Converts samples to beats.
    #[inline]
    pub fn samples_to_beats(&self, samples: SampleTime, sample_rate: f32) -> f32 {
        let seconds = samples as f32 / sample_rate.max(1.0);
        seconds * self.bpm / 60.0
    }
}
