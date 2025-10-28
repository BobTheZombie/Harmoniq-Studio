use std::path::PathBuf;

use crate::state::session::{ChannelId, NoteId, PatternId};

#[derive(Debug, Clone)]
pub enum Command {
    // Channel Rack
    AddPattern,
    SelectPattern(PatternId),
    AddChannelInstrument {
        name: String,
        plugin_uid: String,
    },
    AddChannelSample {
        name: String,
        path: PathBuf,
    },
    RemoveChannel(ChannelId),
    CloneChannel(ChannelId),
    ToggleChannelMute(ChannelId, bool),
    ToggleChannelSolo(ChannelId, bool),
    ConvertStepsToMidi {
        channel_id: ChannelId,
        pattern_id: PatternId,
    },

    // Piano Roll
    OpenPianoRoll {
        channel_id: ChannelId,
        pattern_id: PatternId,
    },
    ClosePianoRoll,
    PianoRollInsertNotes {
        channel_id: ChannelId,
        pattern_id: PatternId,
        notes: Vec<(u32, u32, i8, u8)>,
    },
    PianoRollDeleteNotes {
        channel_id: ChannelId,
        pattern_id: PatternId,
        note_ids: Vec<NoteId>,
    },
    PianoRollSetNoteVelocity {
        channel_id: ChannelId,
        pattern_id: PatternId,
        note_id: NoteId,
        velocity: u8,
    },
}
