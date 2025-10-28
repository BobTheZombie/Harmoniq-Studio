#![warn(missing_docs)]

//! Harmoniq MIDI utilities.

pub mod backend_midir;
pub mod clock;
pub mod config;
pub mod device;
pub mod hotplug;
pub mod learn;

pub use device::{MidiDeviceId, MidiDeviceManager, MidiEvent, MidiMessage, MidiSource};

/// Timestamp captured from the monotonic clock when a MIDI event was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MidiTimestamp {
    /// Nanoseconds since an arbitrary monotonic epoch.
    pub nanos_monotonic: u64,
}
