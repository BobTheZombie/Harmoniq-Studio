use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSeq {
    pub lanes: Vec<StepLane>,
    pub steps_per_lane: usize,
}

impl StepSeq {
    pub fn default_grid() -> Self {
        Self {
            lanes: vec![StepLane::default(); 4],
            steps_per_lane: 16,
        }
    }

    pub fn lane(&self, index: usize) -> Option<&StepLane> {
        self.lanes.get(index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepLane {
    pub steps: Vec<Step>,
}

impl StepLane {
    pub fn step(&self, index: usize) -> Option<&Step> {
        self.steps.get(index)
    }
}

impl Default for StepLane {
    fn default() -> Self {
        Self {
            steps: vec![Step::default(); 16],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub active: bool,
    pub velocity: f32,
}

impl Step {
    pub fn with_velocity(velocity: f32) -> Self {
        Self {
            active: true,
            velocity,
        }
    }
}

impl Default for Step {
    fn default() -> Self {
        Self {
            active: false,
            velocity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoRoll {
    pub notes: Vec<PianoRollNote>,
}

impl PianoRoll {
    pub fn default() -> Self {
        Self { notes: Vec::new() }
    }

    pub fn add_note(&mut self, note: PianoRollNote) {
        self.notes.push(note);
        self.notes.sort_by_key(|note| note.start_samples);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoRollNote {
    pub pitch: u8,
    pub start_samples: u64,
    pub length_samples: u64,
    pub velocity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternLaneRef {
    pub channel_id: u32,
    pub steps: BTreeMap<usize, StepOverride>,
}

impl PatternLaneRef {
    pub fn new(channel_id: u32) -> Self {
        Self {
            channel_id,
            steps: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOverride {
    pub active: Option<bool>,
    pub velocity: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: u32,
    pub name: String,
    pub lanes: Vec<PatternLaneRef>,
}

impl Pattern {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            lanes: Vec::new(),
        }
    }
}
