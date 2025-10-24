use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Utility helpers for converting between musical and time domains.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tempo(pub f64);

impl Tempo {
    #[inline]
    pub fn beats_per_minute(&self) -> f64 {
        self.0
    }

    #[inline]
    pub fn beats_per_second(&self) -> f64 {
        self.0 / 60.0
    }

    #[inline]
    pub fn seconds_per_beat(&self) -> f64 {
        1.0 / self.beats_per_second()
    }

    #[inline]
    pub fn samples_per_beat(&self, sample_rate: f32) -> f64 {
        self.seconds_per_beat() * sample_rate as f64
    }
}

impl Default for Tempo {
    fn default() -> Self {
        Self(120.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn beats_per_bar(&self) -> u32 {
        self.numerator.max(1) as u32
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::four_four()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transport {
    pub tempo: Tempo,
    pub time_signature: TimeSignature,
    pub sample_position: u64,
    pub is_playing: bool,
    pub map_version: u64,
    pub tempo_map: SharedTempoMap,
}

impl Transport {
    pub fn samples_per_beat(&self, sample_rate: f32) -> f64 {
        self.tempo.samples_per_beat(sample_rate)
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            tempo: Tempo::default(),
            time_signature: TimeSignature::default(),
            sample_position: 0,
            is_playing: false,
            map_version: 0,
            tempo_map: Arc::new(TempoMap::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub start: u64,
    pub end: u64,
}

impl LoopRegion {
    pub fn length(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoSegment {
    pub start_sample: u64,
    pub tempo: Tempo,
    pub time_signature: TimeSignature,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoMap {
    segments: Vec<TempoSegment>,
}

impl TempoMap {
    pub fn new(mut segments: Vec<TempoSegment>) -> Self {
        if segments.is_empty() {
            segments.push(TempoSegment {
                start_sample: 0,
                tempo: Tempo::default(),
                time_signature: TimeSignature::default(),
            });
        }
        segments.sort_by(|a, b| a.start_sample.cmp(&b.start_sample));
        if segments.first().map_or(true, |seg| seg.start_sample != 0) {
            let first = segments.first().cloned().unwrap_or(TempoSegment {
                start_sample: 0,
                tempo: Tempo::default(),
                time_signature: TimeSignature::default(),
            });
            if first.start_sample != 0 {
                segments.insert(
                    0,
                    TempoSegment {
                        start_sample: 0,
                        tempo: first.tempo,
                        time_signature: first.time_signature,
                    },
                );
            }
        }
        segments.dedup_by(|a, b| {
            if a.start_sample == b.start_sample {
                b.tempo = a.tempo;
                b.time_signature = a.time_signature;
                true
            } else {
                false
            }
        });
        Self { segments }
    }

    pub fn single(tempo: Tempo, signature: TimeSignature) -> Self {
        Self::new(vec![TempoSegment {
            start_sample: 0,
            tempo,
            time_signature: signature,
        }])
    }

    pub fn segments(&self) -> &[TempoSegment] {
        &self.segments
    }

    pub fn segment_index_at(&self, sample: u64) -> usize {
        match self
            .segments
            .binary_search_by(|segment| segment.start_sample.cmp(&sample))
        {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        }
    }

    pub fn segment_at(&self, sample: u64) -> &TempoSegment {
        let idx = self.segment_index_at(sample);
        self.segments
            .get(idx)
            .unwrap_or(self.segments.last().expect("tempo map segment"))
    }

    pub fn tempo_at(&self, sample: u64) -> Tempo {
        self.segment_at(sample).tempo
    }

    pub fn time_signature_at(&self, sample: u64) -> TimeSignature {
        self.segment_at(sample).time_signature
    }

    pub fn next_change_after(&self, sample: u64) -> Option<u64> {
        let idx = self.segment_index_at(sample);
        self.segments
            .get(idx + 1)
            .map(|segment| segment.start_sample)
    }

    pub fn first_beat_at_or_after(&self, sample_rate: f32, sample: u64) -> Option<BeatInfo> {
        let mut index = self.segment_index_at(sample);
        loop {
            let segment = self.segments.get(index)?;
            let spb = segment.tempo.samples_per_beat(sample_rate);
            let start_sample = segment.start_sample as f64;
            let next_boundary = self
                .segments
                .get(index + 1)
                .map(|seg| seg.start_sample as f64)
                .unwrap_or(f64::INFINITY);
            let relative = (sample as f64 - start_sample).max(0.0);
            let mut beat_in_segment = if relative == 0.0 {
                0.0
            } else {
                (relative / spb).ceil()
            };
            let mut beat_sample = start_sample + beat_in_segment * spb;
            if beat_sample >= next_boundary {
                index += 1;
                continue;
            }
            if beat_sample < start_sample {
                beat_sample = start_sample;
                beat_in_segment = 0.0;
            }
            let offset = self.segment_beat_offset(sample_rate, index);
            let beat_index = offset + beat_in_segment.round() as u64;
            return Some(BeatInfo {
                sample: beat_sample.round() as u64,
                beat_index,
                time_signature: segment.time_signature,
            });
        }
    }

    pub fn beat_after(&self, sample_rate: f32, beat: &BeatInfo) -> Option<BeatInfo> {
        let sample = beat.sample.saturating_add(1);
        self.first_beat_at_or_after(sample_rate, sample)
    }

    fn segment_beat_offset(&self, sample_rate: f32, segment_index: usize) -> u64 {
        let mut beats = 0.0;
        for window in self.segments.windows(2).take(segment_index) {
            let current = &window[0];
            let next = &window[1];
            let spb = current.tempo.samples_per_beat(sample_rate);
            let len = next.start_sample.saturating_sub(current.start_sample) as f64;
            beats += len / spb;
        }
        beats.round() as u64
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::new(vec![TempoSegment {
            start_sample: 0,
            tempo: Tempo::default(),
            time_signature: TimeSignature::default(),
        }])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatInfo {
    pub sample: u64,
    pub beat_index: u64,
    pub time_signature: TimeSignature,
}

impl BeatInfo {
    pub fn is_downbeat(&self) -> bool {
        (self.beat_index % self.time_signature.beats_per_bar() as u64) == 0
    }
}

pub type SharedTempoMap = Arc<TempoMap>;
