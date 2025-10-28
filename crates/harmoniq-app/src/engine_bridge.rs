use crate::state::session::{PatternId, Session};

pub struct MidiEmitter;

impl MidiEmitter {
    #[allow(dead_code)]
    pub fn pump(session: &Session, pattern_id: PatternId, pos_ticks: u32, window_ticks: u32) {
        if let Some(pattern) = session.pattern(pattern_id) {
            for channel in &session.channels {
                if let Some(clip) = pattern.clip(channel.id) {
                    for note in clip.notes.values() {
                        if note.start_ticks >= pos_ticks
                            && note.start_ticks < pos_ticks.saturating_add(window_ticks)
                        {
                            // TODO: send note on/off messages to engine bridge.
                            // This lives on the non-realtime thread. Avoid pushing work onto the
                            // realtime audio callback from here.
                            let _ = (note.start_ticks, note.length_ticks);
                        }
                    }
                }
            }
        }
    }
}
