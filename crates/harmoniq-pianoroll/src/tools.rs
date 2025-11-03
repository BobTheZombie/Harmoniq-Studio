use std::collections::HashMap;

use egui::{Modifiers, Pos2, Rect, Vec2};

use crate::model::{grid::Snapper, Clip, Edit, EditorState, Note, SnapUnit};

/// Tools available in the piano roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Arrow,
    Draw,
    Erase,
    Split,
    Glue,
    Line,
    Curve,
    Mute,
    Quantize,
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Arrow
    }
}

/// Hit-test information for a note under the pointer.
#[derive(Clone, Debug)]
pub struct HitNote {
    pub id: u64,
    pub rect: Rect,
}

/// Pointer position expressed in editor units.
#[derive(Clone, Copy, Debug)]
pub struct PointerPosition {
    pub pos: Pos2,
    pub time_ppq: i64,
    pub pitch: u8,
}

#[derive(Clone, Debug)]
enum GestureState {
    DragNotes {
        ids: Vec<u64>,
        origin: HashMap<u64, (i64, u8)>,
        start_pointer: PointerPosition,
    },
    DrawNote {
        id: u64,
        start_pointer: PointerPosition,
    },
    Marquee {
        start_pos: Pos2,
        current: Pos2,
    },
    Pan {
        start_pos: Pos2,
        start_scroll: Vec2,
    },
}

/// Result of processing pointer events.
#[derive(Default)]
pub struct ToolOutput {
    pub edits: Vec<Edit>,
    pub selection: Option<Vec<u64>>,
    pub marquee: Option<Rect>,
    pub request_pan: Option<Vec2>,
}

/// Manages gesture state for the different editing tools.
pub struct ToolController {
    pub active: Tool,
    gesture: Option<GestureState>,
    pub snapper: Snapper,
}

impl ToolController {
    pub fn new(ppq: i32, snap: Option<SnapUnit>, triplets: bool) -> Self {
        Self {
            active: Tool::Arrow,
            gesture: None,
            snapper: Snapper::new(ppq, snap, triplets, 0.0, crate::model::Timebase::Musical),
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        if self.active != tool {
            self.gesture = None;
        }
        self.active = tool;
    }

    pub fn update_snapper(&mut self, ppq: i32, snap: Option<SnapUnit>, triplets: bool, swing: f32) {
        self.snapper = Snapper::new(ppq, snap, triplets, swing, crate::model::Timebase::Musical);
    }

    pub fn on_pointer_pressed(
        &mut self,
        ctx: &mut EditorState,
        pointer: PointerPosition,
        hit: Option<HitNote>,
        modifiers: Modifiers,
    ) -> ToolOutput {
        let mut output = ToolOutput::default();
        match self.active {
            Tool::Arrow => {
                if modifiers.command || modifiers.ctrl {
                    self.gesture = Some(GestureState::Pan {
                        start_pos: pointer.pos,
                        start_scroll: ctx.scroll_px,
                    });
                    return output;
                }
                if let Some(hit) = hit {
                    let mut ids = ctx.selection.clone();
                    if !ids.contains(&hit.id) {
                        if modifiers.shift {
                            ids.push(hit.id);
                        } else {
                            ids = vec![hit.id];
                        }
                    }
                    ctx.clear_selection();
                    for id in &ids {
                        ctx.select_note(*id, true);
                    }
                    let mut origin = HashMap::new();
                    for note in &ctx.clip.notes {
                        if ids.contains(&note.id) {
                            origin.insert(note.id, (note.start_ppq, note.pitch));
                        }
                    }
                    self.gesture = Some(GestureState::DragNotes {
                        ids: ids.clone(),
                        origin,
                        start_pointer: pointer,
                    });
                    output.selection = Some(ids);
                } else {
                    self.gesture = Some(GestureState::Marquee {
                        start_pos: pointer.pos,
                        current: pointer.pos,
                    });
                    ctx.clear_selection();
                    output.selection = Some(Vec::new());
                }
            }
            Tool::Draw => {
                let new_id = ctx.next_note_id();
                let length = (ctx.ppq() / 4).max(1) as i64;
                let start = self.snapper.snap_ppq(pointer.time_ppq);
                let mut note = Note {
                    id: new_id,
                    start_ppq: start,
                    dur_ppq: length,
                    pitch: pointer.pitch,
                    vel: 100,
                    chan: 0,
                    selected: false,
                };
                if modifiers.shift {
                    note.start_ppq = pointer.time_ppq;
                }
                ctx.clip.notes.push(note.clone());
                ctx.clip.sort_notes();
                self.gesture = Some(GestureState::DrawNote {
                    id: new_id,
                    start_pointer: pointer,
                });
                output.edits.push(Edit::Add(note));
            }
            Tool::Erase => {
                if let Some(hit) = hit {
                    output.edits.push(Edit::Remove(hit.id));
                }
            }
            Tool::Split => {
                if let Some(hit) = hit {
                    if let Some(note) = ctx.clip.notes.iter_mut().find(|n| n.id == hit.id) {
                        if let Some(new_note) = crate::model::split(note, pointer.time_ppq) {
                            output.edits.push(Edit::Update {
                                id: hit.id,
                                start_ppq: note.start_ppq,
                                dur_ppq: note.dur_ppq,
                                pitch: note.pitch,
                                vel: note.vel,
                                chan: note.chan,
                            });
                            ctx.clip.notes.push(new_note.clone());
                            output.edits.push(Edit::Add(new_note));
                        }
                    }
                }
            }
            Tool::Glue => {
                crate::model::glue(&mut ctx.clip.notes);
            }
            Tool::Line | Tool::Curve | Tool::Mute | Tool::Quantize => {
                // Not yet implemented in the interactive controller; reserved for future work.
            }
        }
        output
    }

    pub fn on_pointer_dragged(
        &mut self,
        ctx: &mut EditorState,
        pointer: PointerPosition,
        modifiers: Modifiers,
    ) -> ToolOutput {
        let mut output = ToolOutput::default();
        if let Some(gesture) = &mut self.gesture {
            match gesture {
                GestureState::DragNotes {
                    ids,
                    origin,
                    start_pointer,
                } => {
                    let delta_time = if modifiers.shift {
                        pointer.time_ppq - start_pointer.time_ppq
                    } else {
                        self.snapper.snap_ppq(pointer.time_ppq)
                            - self.snapper.snap_ppq(start_pointer.time_ppq)
                    };
                    let delta_pitch = if modifiers.alt {
                        0
                    } else {
                        (pointer.pitch as i32 - start_pointer.pitch as i32) as i64
                    };
                    for id in ids.iter().copied() {
                        if let Some(note) = ctx.clip.notes.iter_mut().find(|n| n.id == id) {
                            if let Some((start_ppq, pitch)) = origin.get(&id) {
                                note.start_ppq = start_ppq + delta_time;
                                let new_pitch = (*pitch as i64 + delta_pitch).clamp(0, 127) as u8;
                                note.pitch = new_pitch;
                                output.edits.push(Edit::Update {
                                    id,
                                    start_ppq: note.start_ppq,
                                    dur_ppq: note.dur_ppq,
                                    pitch: note.pitch,
                                    vel: note.vel,
                                    chan: note.chan,
                                });
                            }
                        }
                    }
                }
                GestureState::DrawNote { id, start_pointer } => {
                    let len = (pointer.time_ppq - start_pointer.time_ppq).max(1);
                    if let Some(note) = ctx.clip.notes.iter_mut().find(|n| n.id == *id) {
                        note.dur_ppq = if modifiers.shift {
                            len
                        } else {
                            let snapped_start = self.snapper.snap_ppq(start_pointer.time_ppq);
                            let snapped_end = self.snapper.snap_ppq(pointer.time_ppq);
                            (snapped_end - snapped_start).max(1)
                        };
                        note.pitch = pointer.pitch;
                        output.edits.push(Edit::Update {
                            id: *id,
                            start_ppq: note.start_ppq,
                            dur_ppq: note.dur_ppq,
                            pitch: note.pitch,
                            vel: note.vel,
                            chan: note.chan,
                        });
                    }
                }
                GestureState::Marquee { start_pos, current } => {
                    *current = pointer.pos;
                    output.marquee = Some(Rect::from_two_pos(*start_pos, *current));
                }
                GestureState::Pan {
                    start_pos,
                    start_scroll,
                } => {
                    let delta = pointer.pos - *start_pos;
                    output.request_pan = Some(*start_scroll - delta);
                }
            }
        }
        output
    }

    pub fn on_pointer_released(&mut self) {
        self.gesture = None;
    }
}

/// Convert a pointer position into PPQ time based on zoom and scroll.
pub fn pointer_to_ppq(clip: &Clip, zoom_x: f32, scroll_x: f32, pointer: f32) -> i64 {
    let beats = ((pointer + scroll_x) / zoom_x).max(0.0);
    (beats * clip.ppq() as f32).round() as i64
}

/// Convert a Y coordinate to MIDI pitch.
pub fn pointer_to_pitch(zoom_y: f32, scroll_y: f32, rect: Rect, pointer: f32) -> u8 {
    let local = rect.bottom() - pointer + scroll_y;
    let pitch = (local / zoom_y).round() as i32;
    pitch.clamp(0, 127) as u8
}
