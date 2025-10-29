use std::collections::BTreeMap;

pub type NoteId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiNote {
    pub id: NoteId,
    pub start_ticks: u32,
    pub length_ticks: u32,
    pub key: i8,
    pub velocity: u8,
}

#[derive(Clone, Debug, Default)]
pub struct MidiClip {
    pub ppq: u32,
    pub notes: BTreeMap<NoteId, MidiNote>,
    next_id: NoteId,
}

impl MidiClip {
    pub fn new(ppq: u32) -> Self {
        Self {
            ppq,
            ..Default::default()
        }
    }

    pub fn insert(&mut self, mut note: MidiNote) -> NoteId {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        note.id = id;
        self.notes.insert(id, note);
        id
    }

    pub fn remove_many(&mut self, ids: &[NoteId]) {
        for id in ids {
            self.notes.remove(id);
        }
    }

    pub fn clear(&mut self) {
        self.notes.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Draw,
    Select,
    Slice,
}

#[derive(Clone, Debug)]
pub struct PianoRollState {
    pub clip: MidiClip,
    pub tool: Tool,
    pub bar_len: u32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub selection: Vec<NoteId>,
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            clip: MidiClip::new(480),
            tool: Tool::Draw,
            bar_len: 4,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 24.0,
            zoom_y: 14.0,
            selection: vec![],
        }
    }
}
