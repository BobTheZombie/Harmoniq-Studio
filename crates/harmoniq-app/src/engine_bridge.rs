use crate::state::session::{MidiClip, PatternId, Session};

pub struct MidiEmitter;

impl MidiEmitter {
    pub fn pump(
        session: &Session,
        pattern_id: PatternId,
        _pos_ticks: u32,
        _window_ticks: u32,
    ) -> Vec<(u32, u32, i8, u8)> {
        let mut emitted = Vec::new();
        let pattern = session
            .patterns
            .iter()
            .find(|pattern| pattern.id == pattern_id);
        if let Some(pattern) = pattern {
            for clip in pattern.clip_per_channel.values() {
                emitted.extend(Self::emit_from_clip(clip));
            }
        }
        // TODO: send note on/off events to the audio engine once available.
        emitted
    }

    fn emit_from_clip(clip: &MidiClip) -> Vec<(u32, u32, i8, u8)> {
        clip.notes
            .values()
            .map(|note| (note.start_ticks, note.length_ticks, note.key, note.velocity))
            .collect()
    }
}
