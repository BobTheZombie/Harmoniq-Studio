#![warn(missing_docs)]

//! Harmoniq MIDI utilities.

/// Midir-based backend implementation.
pub mod backend_midir;
/// Timing utilities for MIDI processing.
pub mod clock;
/// Serialization helpers for MIDI configuration.
pub mod config;
/// Device management and backend coordination primitives.
pub mod device;
/// MIDI hotplug monitoring helpers.
pub mod hotplug;
/// MIDI learn utilities for mapping parameters.
pub mod learn;
/// MIDI output helpers.
pub mod output;

pub use device::{MidiDeviceId, MidiDeviceManager, MidiEvent, MidiMessage, MidiSource};
pub use output::{MidiOutputHandle, MidiOutputManager};

/// Timestamp captured from the monotonic clock when a MIDI event was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MidiTimestamp {
    /// Nanoseconds since an arbitrary monotonic epoch.
    pub nanos_monotonic: u64,
}
