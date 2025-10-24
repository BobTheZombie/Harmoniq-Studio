use serde::{Deserialize, Serialize};

use crate::pattern::{Pattern, PianoRoll, StepSeq};

/// A reference to a plugin entry stored in the plugin database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRef {
    pub id: String,
    pub format: PluginFormat,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginFormat {
    Clap,
    Vst3,
    Harmoniq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rack {
    pub swing: f32,
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
}

impl Rack {
    pub fn new() -> Self {
        Self {
            swing: 0.0,
            channels: Vec::new(),
            patterns: Vec::new(),
        }
    }

    pub fn find_channel(&self, id: u32) -> Option<&Channel> {
        self.channels.iter().find(|ch| ch.id == id)
    }

    pub fn find_pattern(&self, id: u32) -> Option<&Pattern> {
        self.patterns.iter().find(|pat| pat.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: u32,
    pub name: String,
    pub color: [f32; 4],
    pub is_instrument: bool,
    pub plugin_ref: Option<PluginRef>,
    pub mixer_track: u32,
    pub steps: StepSeq,
    pub piano_roll: PianoRoll,
    pub inserts: [Option<PluginRef>; 10],
}

impl Channel {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color: [1.0, 1.0, 1.0, 1.0],
            is_instrument: true,
            plugin_ref: None,
            mixer_track: 0,
            steps: StepSeq::default_grid(),
            piano_roll: PianoRoll::default(),
            inserts: Default::default(),
        }
    }
}
