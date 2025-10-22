//! Real-time safe transport and tempo map support.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoEvent {
    pub position_samples: u64,
    pub bpm: f64,
    pub time_signature_numerator: u8,
    pub time_signature_denominator: u8,
}

impl Default for TempoEvent {
    fn default() -> Self {
        Self {
            position_samples: 0,
            bpm: 120.0,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoMap {
    sample_rate: u32,
    events: Vec<TempoEvent>,
}

impl TempoMap {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            events: vec![TempoEvent::default()],
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
    }

    pub fn events(&self) -> &[TempoEvent] {
        &self.events
    }

    pub fn push_event(&mut self, event: TempoEvent) {
        self.events.push(event);
        self.events.sort_by_key(|event| event.position_samples);
    }

    pub fn tempo_at(&self, position_samples: u64) -> TempoEvent {
        self.events
            .iter()
            .rev()
            .find(|event| event.position_samples <= position_samples)
            .cloned()
            .unwrap_or_else(|| self.events.first().cloned().unwrap_or_default())
    }

    pub fn beats_per_sample(&self, position_samples: u64) -> f64 {
        let tempo = self.tempo_at(position_samples);
        tempo.bpm / 60.0 / self.sample_rate as f64
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TransportCommand {
    Play,
    Stop,
    SetPosition { samples: u64 },
    SetTempo { bpm: f64 },
}

#[derive(Debug, Clone)]
pub struct TransportState {
    playing: bool,
    position_samples: u64,
    tempo_map: TempoMap,
}

impl TransportState {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            playing: false,
            position_samples: 0,
            tempo_map: TempoMap::new(sample_rate),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.tempo_map.sample_rate()
    }

    pub fn tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    pub fn tempo_map_mut(&mut self) -> &mut TempoMap {
        &mut self.tempo_map
    }

    pub fn apply(&mut self, command: TransportCommand) {
        match command {
            TransportCommand::Play => self.playing = true,
            TransportCommand::Stop => self.playing = false,
            TransportCommand::SetPosition { samples } => self.position_samples = samples,
            TransportCommand::SetTempo { bpm } => {
                self.tempo_map.push_event(TempoEvent {
                    position_samples: self.position_samples,
                    bpm,
                    ..TempoEvent::default()
                });
            }
        }
    }

    pub fn advance(&mut self, frames: u64) {
        if self.playing {
            self.position_samples = self.position_samples.saturating_add(frames);
        }
    }

    pub fn position_samples(&self) -> u64 {
        self.position_samples
    }

    pub fn position_beats(&self) -> f64 {
        let beats_per_sample = self.tempo_map.beats_per_sample(self.position_samples);
        beats_per_sample * self.position_samples as f64
    }
}
