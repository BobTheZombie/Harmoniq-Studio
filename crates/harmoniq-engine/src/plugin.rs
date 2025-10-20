use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AudioBuffer, BufferConfig, ChannelLayout};

/// Unique identifier for a plugin instance within the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub u64);

/// Metadata describing a plugin instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: Option<String>,
    pub description: Option<String>,
}

impl PluginDescriptor {
    pub fn new(id: impl Into<String>, name: impl Into<String>, vendor: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            vendor: vendor.into(),
            version: None,
            description: None,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Simplified MIDI event representation for sequencing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, control: u8, value: u8 },
    PitchBend { channel: u8, lsb: u8, msb: u8 },
}

/// Errors that can be returned by plugin operations.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin reported an invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("plugin failed to process audio: {0}")]
    Processing(String),
    #[error("plugin is not ready to process")]
    NotPrepared,
}

/// Primary audio processor trait implemented by native plugins.
pub trait AudioProcessor: Send + Sync {
    fn descriptor(&self) -> PluginDescriptor;
    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()>;
    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()>;

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

/// Trait for plugins capable of consuming MIDI events.
pub trait MidiProcessor: AudioProcessor {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()>;
}

impl fmt::Display for PluginDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.vendor)
    }
}
