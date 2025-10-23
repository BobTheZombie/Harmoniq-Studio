//! MIDI input abstraction with Linux-first support.

use std::time::Instant;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;

/// Timestamp associated with a MIDI message.
pub type MidiTimestamp = u64;

/// Errors that can be produced while dealing with MIDI backends.
#[derive(Debug, Error)]
pub enum MidiError {
    /// The requested port could not be found.
    #[error("unknown MIDI port")]
    UnknownPort,
    /// Backend specific failure with additional context.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Representation of a MIDI message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MidiMessage {
    /// Raw MIDI bytes.
    pub data: [u8; 3],
    /// Timestamp in microseconds since start.
    pub timestamp: MidiTimestamp,
}

impl MidiMessage {
    /// Creates a message from the given bytes.
    pub fn new(data: [u8; 3], timestamp: MidiTimestamp) -> Self {
        Self { data, timestamp }
    }
}

/// Descriptor describing a MIDI port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiPortDescriptor {
    /// Human readable port name.
    pub name: String,
    /// Whether the port supports MIDI Polyphonic Expression.
    pub mpe_enabled: bool,
}

/// Captures incoming MIDI data and stores it in a ring buffer.
#[derive(Debug)]
pub struct MidiInput {
    start: Instant,
    buffer: Mutex<Vec<MidiMessage>>,
}

impl MidiInput {
    /// Creates a new MIDI input capturing to an internal buffer.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// Pushes an incoming message. In production this would be called by the ALSA callback.
    pub fn push_message(&self, bytes: [u8; 3]) {
        let elapsed = self.start.elapsed().as_micros() as MidiTimestamp;
        self.buffer.lock().push(MidiMessage::new(bytes, elapsed));
    }

    /// Retrieves and clears any buffered messages.
    pub fn drain(&self) -> Vec<MidiMessage> {
        let mut guard = self.buffer.lock();
        let messages = guard.clone();
        guard.clear();
        messages
    }
}

/// Manager responsible for enumerating MIDI devices.
#[derive(Debug, Default)]
pub struct MidiManager {
    ports: Vec<MidiPortDescriptor>,
}

impl MidiManager {
    /// Enumerates available ports on Linux using ALSA when possible.
    pub fn enumerate() -> Self {
        // Placeholder: In a future iteration this will talk to ALSA.
        debug!("enumerating stub MIDI ports");
        Self {
            ports: vec![MidiPortDescriptor {
                name: "Virtual Keyboard".into(),
                mpe_enabled: false,
            }],
        }
    }

    /// Returns all known ports.
    pub fn ports(&self) -> &[MidiPortDescriptor] {
        &self.ports
    }
}
