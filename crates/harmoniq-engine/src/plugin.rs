use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

/// Monotonic timestamp attached to MIDI events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiTimestamp {
    micros: u64,
}

impl MidiTimestamp {
    /// Creates a timestamp representing the current instant relative to the
    /// process wide MIDI clock epoch.
    pub fn now() -> Self {
        Self::from_instant(Instant::now())
    }

    /// Creates a timestamp from a duration since the MIDI clock epoch.
    pub fn from_duration(duration: Duration) -> Self {
        Self {
            micros: duration.as_micros().min(u64::MAX as u128) as u64,
        }
    }

    /// Creates a timestamp from microseconds since the MIDI clock epoch.
    pub fn from_micros(micros: u64) -> Self {
        Self { micros }
    }

    /// Returns the timestamp expressed as a [`Duration`].
    pub fn as_duration(self) -> Duration {
        Duration::from_micros(self.micros)
    }

    /// Absolute difference between two timestamps.
    pub fn abs_diff(self, other: MidiTimestamp) -> Duration {
        let lhs = self.micros;
        let rhs = other.micros;
        if lhs >= rhs {
            Duration::from_micros(lhs - rhs)
        } else {
            Duration::from_micros(rhs - lhs)
        }
    }

    fn from_instant(now: Instant) -> Self {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        let base = EPOCH.get_or_init(|| Instant::now());
        let duration = now
            .checked_duration_since(*base)
            .unwrap_or_else(|| Duration::from_micros(0));
        Self::from_duration(duration)
    }
}

/// Simplified MIDI event representation for sequencing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
        timestamp: MidiTimestamp,
    },
    NoteOff {
        channel: u8,
        note: u8,
        timestamp: MidiTimestamp,
    },
    ControlChange {
        channel: u8,
        control: u8,
        value: u8,
        timestamp: MidiTimestamp,
    },
    PitchBend {
        channel: u8,
        lsb: u8,
        msb: u8,
        timestamp: MidiTimestamp,
    },
}

impl MidiEvent {
    /// Returns the timestamp associated with this event.
    pub fn timestamp(&self) -> MidiTimestamp {
        match self {
            MidiEvent::NoteOn { timestamp, .. }
            | MidiEvent::NoteOff { timestamp, .. }
            | MidiEvent::ControlChange { timestamp, .. }
            | MidiEvent::PitchBend { timestamp, .. } => *timestamp,
        }
    }

    /// Helper to construct a MIDI event from raw bytes.
    ///
    /// The `sample_offset` is interpreted as microseconds to align with
    /// [`MidiTimestamp`], preserving approximate ordering when events are
    /// sorted by time.
    pub fn new(sample_offset: u32, data: [u8; 3]) -> Self {
        let timestamp = MidiTimestamp::from_micros(sample_offset as u64);
        let status = data[0] & 0xF0;
        let channel = data[0] & 0x0F;

        match status {
            0x80 => MidiEvent::NoteOff {
                channel,
                note: data[1],
                timestamp,
            },
            0x90 => MidiEvent::NoteOn {
                channel,
                note: data[1],
                velocity: data[2],
                timestamp,
            },
            0xB0 => MidiEvent::ControlChange {
                channel,
                control: data[1],
                value: data[2],
                timestamp,
            },
            0xE0 => MidiEvent::PitchBend {
                channel,
                lsb: data[1],
                msb: data[2],
                timestamp,
            },
            _ => MidiEvent::NoteOff {
                channel,
                note: data[1],
                timestamp,
            },
        }
    }

    /// Returns the timestamp expressed as an approximate sample offset.
    pub fn sample_offset(&self) -> u32 {
        self.timestamp()
            .as_duration()
            .as_micros()
            .min(u32::MAX as u128) as u32
    }
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

    /// Returns the processing latency in samples introduced by the processor.
    ///
    /// By default processors are assumed to be latency free. Implementations
    /// that introduce look-ahead or oversampling should override this to
    /// provide accurate delay compensation in the engine.
    fn latency_samples(&self) -> usize {
        0
    }

    /// Allows processors to consume queued MIDI events. The default
    /// implementation ignores incoming data which keeps existing
    /// processors backwards compatible without any additional changes.
    fn process_midi(&mut self, _events: &[MidiEvent]) -> anyhow::Result<()> {
        Ok(())
    }

    /// Receives automation changes with sample accurate timing information.
    /// The engine guarantees that offsets never exceed the current audio block
    /// length.
    fn handle_automation_event(
        &mut self,
        _parameter: usize,
        _value: f32,
        _sample_offset: usize,
    ) -> anyhow::Result<()> {
        Ok(())
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
