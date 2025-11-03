use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;

use egui::Vec2;
use smallvec::SmallVec;

use crate::tools::Tool;

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// Grid snapping unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapUnit {
    Bar,
    Beat,
    /// Divisions per beat (e.g. 4 = sixteenth notes).
    Grid(u32),
}

impl SnapUnit {
    pub fn divisions_per_bar(self, beats_per_bar: u32) -> u32 {
        match self {
            SnapUnit::Bar => 1,
            SnapUnit::Beat => beats_per_bar,
            SnapUnit::Grid(div) => beats_per_bar * div.max(1),
        }
    }

    pub fn divisions_per_beat(self) -> u32 {
        match self {
            SnapUnit::Bar => 1,
            SnapUnit::Beat => 1,
            SnapUnit::Grid(div) => div.max(1),
        }
    }
}

/// Time representation of the clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timebase {
    Musical,
    Samples,
}

/// Pitch scale configuration used for highlighting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scale {
    pub tonic: u8,
    pub mode: ScaleMode,
}

impl Scale {
    pub fn contains(&self, pitch: u8) -> bool {
        const MAJOR: [bool; 12] = [
            true, false, true, false, true, true, false, true, false, true, false, true,
        ];
        const MINOR: [bool; 12] = [
            true, false, true, true, false, true, false, true, true, false, true, false,
        ];
        let semitone = (pitch % 12) as usize;
        let pattern = match self.mode {
            ScaleMode::Major => &MAJOR,
            ScaleMode::Minor => &MINOR,
        };
        let root = (self.tonic as usize) % 12;
        pattern[(12 + semitone - root) % 12]
    }
}

/// Diatonic mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    Major,
    Minor,
}

/// MIDI note representation stored inside a clip.
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: u64,
    pub start_ppq: i64,
    pub dur_ppq: i64,
    pub pitch: u8,
    pub vel: u8,
    pub chan: u8,
    pub selected: bool,
}

impl Note {
    pub fn end_ppq(&self) -> i64 {
        self.start_ppq + self.dur_ppq.max(1)
    }
}

/// In-memory clip.
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Clip {
    pub ppq: i32,
    pub notes: Vec<Note>,
    pub loop_start_ppq: i64,
    pub loop_len_ppq: i64,
}

impl Clip {
    pub fn new(ppq: i32) -> Self {
        Self {
            ppq,
            notes: Vec::new(),
            loop_start_ppq: 0,
            loop_len_ppq: (ppq as i64) * 4,
        }
    }

    pub fn ppq(&self) -> i32 {
        self.ppq.max(1)
    }

    pub fn beats_per_bar(&self) -> u32 {
        4
    }

    pub fn sort_notes(&mut self) {
        self.notes.sort_by_key(|n| (n.start_ppq, n.pitch));
    }

    pub fn note_map(&self) -> HashMap<u64, usize> {
        let mut out = HashMap::with_capacity(self.notes.len());
        for (index, note) in self.notes.iter().enumerate() {
            out.insert(note.id, index);
        }
        out
    }
}

/// Automation point inside a controller lane.
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct ControllerPoint {
    pub ppq: i64,
    pub value: f32,
}

impl ControllerPoint {
    pub fn clamp(&mut self) {
        if !self.value.is_finite() {
            self.value = 0.0;
        }
        self.value = self.value.clamp(0.0, 1.0);
    }
}

#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneKind {
    Velocity,
    CC(u8),
    PitchBend,
    ModWheel,
    Aftertouch,
    MPEY,
}

/// Controller lane data.
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Lane {
    pub kind: LaneKind,
    pub points: Vec<ControllerPoint>,
    pub visible: bool,
    pub height: f32,
}

impl Lane {
    pub fn new(kind: LaneKind) -> Self {
        Self {
            kind,
            points: Vec::new(),
            visible: true,
            height: 72.0,
        }
    }
}

/// Definition for quantize presets used by the toolbar and edits.
#[derive(Clone, Debug)]
pub struct QuantizePreset {
    pub name: String,
    pub snap: SnapUnit,
    pub strength: f32,
    pub swing: f32,
    pub range: RangeInclusive<i64>,
    pub iterative: bool,
}

impl QuantizePreset {
    pub fn straight(name: impl Into<String>, snap: SnapUnit) -> Self {
        Self {
            name: name.into(),
            snap,
            strength: 1.0,
            swing: 0.0,
            range: 0..=i64::MAX,
            iterative: false,
        }
    }
}

/// Undoable edit operations emitted by the editor UI.
#[derive(Clone, Debug)]
pub enum Edit {
    Add(Note),
    Remove(u64),
    Update {
        id: u64,
        start_ppq: i64,
        dur_ppq: i64,
        pitch: u8,
        vel: u8,
        chan: u8,
    },
    Quantize {
        preset: QuantizePreset,
        strength: f32,
        swing: f32,
    },
    ControllerChange {
        lane: LaneKind,
        points: Vec<ControllerPoint>,
    },
    LoopChanged {
        start_ppq: i64,
        len_ppq: i64,
    },
}

/// High level editor state shared with the UI.
#[derive(Clone, Debug)]
pub struct EditorState {
    pub clip: Clip,
    pub lanes: Vec<Lane>,
    pub playhead_ppq: i64,
    pub follow_playhead: bool,
    pub snap: Option<SnapUnit>,
    pub triplets: bool,
    pub timebase: Timebase,
    pub key_sig: (u8, u8),
    pub scale_highlight: Option<Scale>,
    pub ghost_clip: Option<Clip>,
    pub selection: Vec<u64>,
    pub tool: Tool,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub scroll_px: Vec2,
    pub quantize_strength: f32,
    pub quantize_swing: f32,
    pub step_input: bool,
    pub follow_zoom: bool,
    history: History,
}

impl EditorState {
    pub fn new(clip: Clip) -> Self {
        Self {
            clip,
            lanes: vec![Lane::new(LaneKind::Velocity)],
            playhead_ppq: 0,
            follow_playhead: false,
            snap: Some(SnapUnit::Grid(4)),
            triplets: false,
            timebase: Timebase::Musical,
            key_sig: (0, 0),
            scale_highlight: None,
            ghost_clip: None,
            selection: Vec::new(),
            tool: Tool::Arrow,
            zoom_x: 48.0,
            zoom_y: 12.0,
            scroll_px: Vec2::ZERO,
            quantize_strength: 1.0,
            quantize_swing: 0.0,
            step_input: false,
            follow_zoom: true,
            history: History::new(200),
        }
    }

    pub fn ppq(&self) -> i32 {
        self.clip.ppq()
    }

    pub fn beats_per_bar(&self) -> u32 {
        self.clip.beats_per_bar()
    }

    pub fn selected_notes_mut(&mut self) -> Vec<&mut Note> {
        let ids: HashSet<u64> = self.selection.iter().copied().collect();
        self.clip
            .notes
            .iter_mut()
            .filter(|note| ids.contains(&note.id))
            .collect()
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        for note in &mut self.clip.notes {
            note.selected = false;
        }
    }

    pub fn select_note(&mut self, id: u64, additive: bool) {
        if !additive {
            self.clear_selection();
        }
        if !self.selection.contains(&id) {
            self.selection.push(id);
        }
        if let Some(note) = self.clip.notes.iter_mut().find(|note| note.id == id) {
            note.selected = true;
        }
    }

    pub fn next_note_id(&self) -> u64 {
        self.clip
            .notes
            .iter()
            .map(|n| n.id)
            .max()
            .unwrap_or(0)
            .wrapping_add(1)
    }

    pub fn apply_edits(&mut self, edits: &[Edit]) {
        for edit in edits {
            match edit {
                Edit::Add(note) => {
                    self.clip.notes.push(note.clone());
                }
                Edit::Remove(id) => {
                    if let Some(pos) = self.clip.notes.iter().position(|n| n.id == *id) {
                        self.clip.notes.remove(pos);
                    }
                }
                Edit::Update {
                    id,
                    start_ppq,
                    dur_ppq,
                    pitch,
                    vel,
                    chan,
                } => {
                    if let Some(note) = self.clip.notes.iter_mut().find(|n| n.id == *id) {
                        note.start_ppq = *start_ppq;
                        note.dur_ppq = *dur_ppq;
                        note.pitch = *pitch;
                        note.vel = *vel;
                        note.chan = *chan;
                    }
                }
                Edit::Quantize {
                    preset,
                    strength,
                    swing,
                } => {
                    let triplets = self.triplets;
                    let ppq = self.ppq();
                    quantize::apply(
                        &mut self.clip.notes,
                        preset,
                        *strength,
                        *swing,
                        preset.range.clone(),
                        ppq,
                        triplets,
                    );
                }
                Edit::ControllerChange { lane, points } => {
                    if let Some(l) = self.lanes.iter_mut().find(|l| &l.kind == lane) {
                        l.points = points.clone();
                        for point in &mut l.points {
                            point.clamp();
                        }
                    }
                }
                Edit::LoopChanged { start_ppq, len_ppq } => {
                    self.clip.loop_start_ppq = *start_ppq;
                    self.clip.loop_len_ppq = (*len_ppq).max(1);
                }
            }
        }
        self.clip.sort_notes();
    }

    pub fn register_history(&mut self, edits: Vec<Edit>) {
        if edits.is_empty() {
            return;
        }
        self.history.push(edits);
    }

    pub fn undo(&mut self) -> Option<Vec<Edit>> {
        self.history.undo()
    }

    pub fn redo(&mut self) -> Option<Vec<Edit>> {
        self.history.redo()
    }
}

/// Simple undo/redo history storing edit batches.
#[derive(Clone, Debug)]
pub struct History {
    undo: SmallVec<[Vec<Edit>; 8]>,
    redo: SmallVec<[Vec<Edit>; 8]>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            undo: SmallVec::new(),
            redo: SmallVec::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn push(&mut self, edits: Vec<Edit>) {
        if self.undo.len() >= self.capacity {
            if !self.undo.is_empty() {
                self.undo.remove(0);
            }
        }
        self.undo.push(edits);
        self.redo.clear();
    }

    pub fn undo(&mut self) -> Option<Vec<Edit>> {
        let edits = self.undo.pop()?;
        self.redo.push(edits.clone());
        Some(edits)
    }

    pub fn redo(&mut self) -> Option<Vec<Edit>> {
        let edits = self.redo.pop()?;
        self.undo.push(edits.clone());
        Some(edits)
    }
}

/// Functions for snapping PPQ values to the current grid.
pub mod grid {
    use super::{SnapUnit, Timebase};

    #[derive(Clone, Debug)]
    pub struct Snapper {
        ppq_per_beat: i32,
        snap: Option<SnapUnit>,
        triplets: bool,
        swing: f32,
        timebase: Timebase,
    }

    impl Snapper {
        pub fn new(
            ppq_per_beat: i32,
            snap: Option<SnapUnit>,
            triplets: bool,
            swing: f32,
            timebase: Timebase,
        ) -> Self {
            Self {
                ppq_per_beat: ppq_per_beat.max(1),
                snap,
                triplets,
                swing,
                timebase,
            }
        }

        pub fn snap_ppq(&self, value: i64) -> i64 {
            let snap = match self.snap {
                None => return value,
                Some(s) => s,
            };
            let base_div = snap.divisions_per_beat();
            let mut step_ppq = (self.ppq_per_beat as i64) / (base_div as i64);
            if self.triplets {
                step_ppq = (self.ppq_per_beat as i64) * 2 / (base_div as i64 * 3).max(1);
            }
            step_ppq = step_ppq.max(1);
            let remainder = value % step_ppq;
            let mut snapped = if remainder.abs() * 2 >= step_ppq.abs() {
                value + (step_ppq - remainder)
            } else {
                value - remainder
            };
            if matches!(self.timebase, Timebase::Musical) {
                // Apply swing: shift every other subdivision forward by swing ratio.
                if self.swing.abs() > f32::EPSILON {
                    let beat_idx = ((snapped / step_ppq) % 2).abs();
                    if beat_idx == 1 {
                        let swing_offset =
                            (self.swing.clamp(-1.0, 1.0) * step_ppq as f32 * 0.5) as i64;
                        snapped += swing_offset;
                    }
                }
            }
            snapped
        }
    }
}

/// Quantize operations on notes.
pub mod quantize {
    use std::ops::RangeInclusive;

    use super::{grid::Snapper, Note, QuantizePreset, Timebase};

    pub fn apply(
        notes: &mut [Note],
        preset: &QuantizePreset,
        strength: f32,
        swing: f32,
        range: RangeInclusive<i64>,
        ppq_per_beat: i32,
        triplets: bool,
    ) {
        if notes.is_empty() {
            return;
        }
        let snapper = Snapper::new(
            ppq_per_beat,
            Some(preset.snap),
            triplets,
            swing,
            Timebase::Musical,
        );
        let amount = strength.clamp(0.0, 1.0);
        for note in notes {
            if !range.contains(&note.start_ppq) {
                continue;
            }
            let snapped = snapper.snap_ppq(note.start_ppq);
            let delta = snapped - note.start_ppq;
            let applied = (delta as f32 * amount).round() as i64;
            note.start_ppq += applied;
            if note.dur_ppq < 1 {
                note.dur_ppq = 1;
            }
        }
    }
}

/// Glue adjacent/overlapping notes sharing pitch and channel.
pub fn glue(notes: &mut Vec<Note>) {
    if notes.is_empty() {
        return;
    }
    let mut last_idx: Option<usize> = None;
    for idx in 0..notes.len() {
        if let Some(prev_idx) = last_idx {
            let (left, right) = notes.split_at_mut(idx);
            let current = &mut right[0];
            let previous = &mut left[prev_idx];
            if current.pitch == previous.pitch && current.chan == previous.chan {
                let prev_end = previous.end_ppq();
                if current.start_ppq <= prev_end {
                    let extension = current.end_ppq().max(prev_end);
                    previous.dur_ppq = extension - previous.start_ppq;
                    current.dur_ppq = 0;
                }
            }
        }
        last_idx = Some(idx);
    }
    notes.retain(|n| n.dur_ppq > 0);
}

/// Split a note at the specified PPQ position, returning the right hand side.
pub fn split(note: &mut Note, split_ppq: i64) -> Option<Note> {
    if split_ppq <= note.start_ppq || split_ppq >= note.end_ppq() {
        return None;
    }
    let right_len = note.end_ppq() - split_ppq;
    note.dur_ppq = split_ppq - note.start_ppq;
    let mut rhs = note.clone();
    rhs.start_ppq = split_ppq;
    rhs.dur_ppq = right_len;
    rhs.id = rhs.id.wrapping_add(1);
    Some(rhs)
}
