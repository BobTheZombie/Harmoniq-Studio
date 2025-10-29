//! Utilities to convert step lanes to a simple MIDI clip representation
//! (kept UI-side; engine bridge does the scheduling later).

use crate::state::{ChannelId, ChannelKind, PatternId, RackState};

#[derive(Clone, Debug)]
pub struct MidiNote {
    pub start_ticks: u32,
    pub length_ticks: u32,
    pub key: i8,
    pub velocity: u8,
}

#[derive(Clone, Debug, Default)]
pub struct MidiClip {
    pub ppq: u32,
    pub notes: Vec<MidiNote>,
}

/// Generate a 1-bar clip from steps using 16th/32nd grid at C4.
pub fn steps_to_midi(state: &RackState, pat: PatternId, ch: ChannelId) -> Option<MidiClip> {
    let chref = state.channels.iter().find(|c| c.id == ch)?;
    if !matches!(chref.kind, ChannelKind::Instrument | ChannelKind::Sample) {
        return None;
    }
    let steps = chref.steps.get(&pat)?;
    let div = chref.steps_per_bar.max(1);
    let ppq = 480;
    let bar_ticks = 4 * ppq;
    let step_ticks = (bar_ticks as f32 / div as f32).round() as u32;

    let mut clip = MidiClip { ppq, notes: vec![] };
    for (i, st) in steps.iter().enumerate() {
        if !st.on {
            continue;
        }
        let start = i as u32 * step_ticks;
        let len = step_ticks.max(ppq / 8);
        clip.notes.push(MidiNote {
            start_ticks: start,
            length_ticks: len,
            key: 60, // C4
            velocity: st.velocity.min(127).max(1),
        });
    }
    Some(clip)
}
