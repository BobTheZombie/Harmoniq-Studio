use crate::state::{ChannelId, ChannelKind, PatternId, RackState, StepIndex};

#[derive(Clone, Debug, Default)]
pub struct MidiNote {
    pub note: u8,
    pub velocity: u8,
    pub start_step: StepIndex,
    pub duration_steps: StepIndex,
    pub pan: i8,
    pub shift_ticks: i16,
}

#[derive(Clone, Debug, Default)]
pub struct MidiClip {
    pub channel_id: ChannelId,
    pub pattern_id: PatternId,
    pub steps_per_bar: u32,
    pub bars: u32,
    pub notes: Vec<MidiNote>,
}

pub fn steps_to_midi(
    state: &RackState,
    pattern_id: PatternId,
    channel_id: ChannelId,
) -> Option<MidiClip> {
    let pattern = state.patterns.iter().find(|pat| pat.id == pattern_id)?;
    let channel = state.channels.iter().find(|ch| ch.id == channel_id)?;
    let steps = channel.steps.get(&pattern_id)?;

    let total_steps = steps.len() as StepIndex;

    let mut clip = MidiClip {
        channel_id,
        pattern_id,
        steps_per_bar: channel.steps_per_bar,
        bars: pattern.bars,
        notes: Vec::new(),
    };

    let note_number = match channel.kind {
        ChannelKind::Instrument | ChannelKind::Effect => 60, // C4
        ChannelKind::Sample => 48,                           // C3
    };

    for (idx, step) in steps.iter().enumerate() {
        if !step.on {
            continue;
        }

        clip.notes.push(MidiNote {
            note: note_number,
            velocity: step.velocity,
            start_step: idx as StepIndex,
            duration_steps: 1,
            pan: step.pan,
            shift_ticks: step.shift_ticks,
        });
    }

    if clip.notes.is_empty() && total_steps == 0 {
        None
    } else {
        Some(clip)
    }
}
