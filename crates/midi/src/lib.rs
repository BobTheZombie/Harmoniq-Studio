use engine_rt::EventQueue;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MidiEvent {
    pub timestamp_samples: u64,
    pub message: MidiMessage,
}

pub struct MidiPort {
    queue: EventQueue<MidiEvent>,
}

impl MidiPort {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: EventQueue::new(capacity),
        }
    }

    pub fn push(&self, event: MidiEvent) -> Result<(), engine_rt::QueueError> {
        self.queue.try_push(event)
    }

    pub fn pop(&self) -> Result<MidiEvent, engine_rt::QueueError> {
        self.queue.try_pop()
    }
}
