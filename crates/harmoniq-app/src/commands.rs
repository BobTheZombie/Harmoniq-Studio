use std::fmt;

use crate::state::session::{ChannelId, PatternId};

/// Commands issued by UI widgets towards the core application state.
#[derive(Debug, Clone)]
pub enum Command {
    // Channel rack
    AddPattern,
    SelectPattern(PatternId),
    AddChannelInstrument {
        name: String,
        plugin_uid: Option<String>,
    },
    AddChannelSample {
        name: String,
        path: String,
    },
    RemoveChannel(ChannelId),
    CloneChannel(ChannelId),
    ToggleChannelMute(ChannelId, bool),
    ToggleChannelSolo(ChannelId, bool),
    ConvertStepsToMidi {
        channel_id: ChannelId,
        pattern_id: PatternId,
    },

    // Piano roll
    OpenPianoRoll {
        channel_id: ChannelId,
        pattern_id: PatternId,
    },
    ClosePianoRoll,
    PianoRollInsertNotes {
        channel_id: ChannelId,
        pattern_id: PatternId,
        /// (start_ticks, length_ticks, key, velocity)
        notes: Vec<(u32, u32, i8, u8)>,
    },
    PianoRollDeleteNotes {
        channel_id: ChannelId,
        pattern_id: PatternId,
        note_ids: Vec<u64>,
    },
    PianoRollSetNoteVelocity {
        channel_id: ChannelId,
        pattern_id: PatternId,
        note_id: u64,
        velocity: u8,
    },
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::AddPattern => write!(f, "AddPattern"),
            Command::SelectPattern(id) => write!(f, "SelectPattern({id})"),
            Command::AddChannelInstrument { name, .. } => {
                write!(f, "AddChannelInstrument({name})")
            }
            Command::AddChannelSample { name, .. } => {
                write!(f, "AddChannelSample({name})")
            }
            Command::RemoveChannel(id) => write!(f, "RemoveChannel({id})"),
            Command::CloneChannel(id) => write!(f, "CloneChannel({id})"),
            Command::ToggleChannelMute(id, value) => {
                write!(f, "ToggleChannelMute({id}, {value})")
            }
            Command::ToggleChannelSolo(id, value) => {
                write!(f, "ToggleChannelSolo({id}, {value})")
            }
            Command::ConvertStepsToMidi {
                channel_id,
                pattern_id,
            } => write!(
                f,
                "ConvertStepsToMidi(channel={channel_id}, pattern={pattern_id})"
            ),
            Command::OpenPianoRoll {
                channel_id,
                pattern_id,
            } => write!(
                f,
                "OpenPianoRoll(channel={channel_id}, pattern={pattern_id})"
            ),
            Command::ClosePianoRoll => write!(f, "ClosePianoRoll"),
            Command::PianoRollInsertNotes { notes, .. } => {
                write!(f, "PianoRollInsertNotes(count={})", notes.len())
            }
            Command::PianoRollDeleteNotes { note_ids, .. } => {
                write!(f, "PianoRollDeleteNotes(count={})", note_ids.len())
            }
            Command::PianoRollSetNoteVelocity {
                note_id, velocity, ..
            } => write!(
                f,
                "PianoRollSetNoteVelocity(note={note_id}, velocity={velocity})"
            ),
        }
    }
}
