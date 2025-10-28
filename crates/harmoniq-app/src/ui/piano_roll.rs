use crossbeam_channel::Sender;
use eframe::egui::{self, PointerButton, Pos2, Rect, Vec2};

use crate::commands::Command;
use crate::state::session::{ChannelId, NoteId, PatternId, Session};

const KEY_HEIGHT: f32 = 24.0;
const KEY_COUNT: usize = 24;
const LOW_KEY: i8 = 36; // C2
const STEP_WIDTH: f32 = 28.0;

#[derive(Default, Debug)]
pub struct PianoRollState {
    pub selected_note: Option<NoteId>,
    drag_start: Option<Pos2>,
    drag_end: Option<Pos2>,
}

pub struct PianoRollView<'a> {
    pub session: &'a Session,
    pub channel_id: ChannelId,
    pub pattern_id: PatternId,
    pub state: &'a mut PianoRollState,
    pub command_tx: Sender<Command>,
}

pub fn piano_roll_ui(ui: &mut egui::Ui, mut view: PianoRollView<'_>) {
    let Some(pattern) = view.session.pattern(view.pattern_id) else {
        ui.label("Pattern missing");
        return;
    };

    let Some(channel) = view.session.channel(view.channel_id) else {
        ui.label("Channel missing");
        return;
    };

    ui.horizontal(|ui| {
        ui.heading(format!("Piano Roll – {} / {}", channel.name, pattern.name));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                let _ = view.command_tx.send(Command::ClosePianoRoll);
            }
        });
    });

    ui.add_space(8.0);

    let clip_ppq = view
        .session
        .clip(view.channel_id, view.pattern_id)
        .map(|clip| clip.ppq)
        .unwrap_or(960);
    let ticks_per_step = (clip_ppq * 4) / 16;
    let total_steps = pattern.total_16th_steps();

    let canvas_height = KEY_COUNT as f32 * KEY_HEIGHT;
    let canvas_width = total_steps as f32 * STEP_WIDTH;

    let (response, painter) =
        ui.allocate_painter(Vec2::new(canvas_width, canvas_height), egui::Sense::drag());
    let canvas_rect = response.rect;

    draw_grid(&painter, canvas_rect, total_steps);
    draw_notes(&painter, &view, canvas_rect, ticks_per_step, clip_ppq);

    handle_note_selection(&response, &mut view, canvas_rect, ticks_per_step, clip_ppq);

    if let (Some(start), Some(end)) = (view.state.drag_start, view.state.drag_end) {
        let preview_rect = drag_rect(canvas_rect, start, end);
        painter.rect_filled(
            preview_rect,
            2.0,
            egui::Color32::from_rgba_unmultiplied(120, 180, 250, 80),
        );
    }

    if response.drag_started_by(PointerButton::Primary) {
        if let Some(pos) = response.interact_pointer_pos() {
            view.state.drag_start = Some(pos);
            view.state.drag_end = Some(pos);
        }
    }

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            view.state.drag_end = Some(pos);
        }
    }

    if response.drag_stopped() {
        if let (Some(start), Some(end)) = (view.state.drag_start.take(), view.state.drag_end.take())
        {
            if let Some((start_ticks, length_ticks, key)) =
                drag_to_note(canvas_rect, start, end, ticks_per_step)
            {
                let _ = view.command_tx.send(Command::PianoRollInsertNotes {
                    channel_id: view.channel_id,
                    pattern_id: view.pattern_id,
                    notes: vec![(start_ticks, length_ticks.max(ticks_per_step), key, 100)],
                });
            }
        }
    }

    ui.add_space(8.0);

    velocity_lane(ui, &mut view);
}

fn draw_grid(painter: &egui::Painter, rect: Rect, total_steps: usize) {
    let dark = egui::Color32::from_gray(30);
    let light = egui::Color32::from_gray(45);
    painter.rect_filled(rect, 0.0, dark);

    for row in 0..KEY_COUNT {
        let y = rect.min.y + row as f32 * KEY_HEIGHT;
        let row_rect = Rect::from_min_size(
            Pos2::new(rect.min.x, y),
            Vec2::new(rect.width(), KEY_HEIGHT),
        );
        let color = if is_black_key(row) {
            egui::Color32::from_gray(25)
        } else {
            egui::Color32::from_gray(50)
        };
        painter.rect_filled(row_rect, 0.0, color);
    }

    for step in 0..=total_steps {
        let x = rect.min.x + step as f32 * STEP_WIDTH;
        let color = if step % 4 == 0 {
            egui::Color32::from_gray(120)
        } else {
            light
        };
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            egui::Stroke::new(1.0, color),
        );
    }
}

fn draw_notes(
    painter: &egui::Painter,
    view: &PianoRollView<'_>,
    rect: Rect,
    ticks_per_step: u32,
    ppq: u32,
) {
    if let Some(clip) = view.session.clip(view.channel_id, view.pattern_id) {
        for note in clip.notes.values() {
            if let Some(row) = key_to_row(note.key) {
                let start_step = note.start_ticks / ticks_per_step;
                let length_steps = (note.length_ticks.max(ticks_per_step)) / ticks_per_step;
                let x = rect.min.x + start_step as f32 * STEP_WIDTH;
                let y = rect.min.y + row as f32 * KEY_HEIGHT;
                let w = (length_steps as f32).max(1.0) * STEP_WIDTH;
                let note_rect =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, KEY_HEIGHT - 4.0));
                let color = if Some(note.id) == view.state.selected_note {
                    egui::Color32::from_rgb(200, 120, 50)
                } else {
                    egui::Color32::from_rgb(90, 180, 250)
                };
                painter.rect_filled(note_rect, 2.0, color);
            }
        }
    }

    let _ = ppq; // placeholder for future snapping controls
}

fn handle_note_selection(
    response: &egui::Response,
    view: &mut PianoRollView<'_>,
    rect: Rect,
    ticks_per_step: u32,
    _ppq: u32,
) {
    if response.clicked_by(PointerButton::Primary) {
        if let Some(pos) = response.interact_pointer_pos() {
            if response.dragged() {
                return;
            }
            let mut hit = None;
            if let Some(clip) = view.session.clip(view.channel_id, view.pattern_id) {
                for note in clip.notes.values() {
                    if let Some(row) = key_to_row(note.key) {
                        let start_step = note.start_ticks / ticks_per_step;
                        let length_steps = (note.length_ticks.max(ticks_per_step)) / ticks_per_step;
                        let x = rect.min.x + start_step as f32 * STEP_WIDTH;
                        let y = rect.min.y + row as f32 * KEY_HEIGHT;
                        let w = (length_steps as f32).max(1.0) * STEP_WIDTH;
                        let note_rect =
                            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, KEY_HEIGHT - 4.0));
                        if note_rect.contains(pos) {
                            hit = Some(note.id);
                            break;
                        }
                    }
                }
            }
            view.state.selected_note = hit;
        }
    }

    if response.clicked_by(PointerButton::Secondary) {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(clip) = view.session.clip(view.channel_id, view.pattern_id) {
                for note in clip.notes.values() {
                    if let Some(row) = key_to_row(note.key) {
                        let start_step = note.start_ticks / ticks_per_step;
                        let length_steps = (note.length_ticks.max(ticks_per_step)) / ticks_per_step;
                        let x = rect.min.x + start_step as f32 * STEP_WIDTH;
                        let y = rect.min.y + row as f32 * KEY_HEIGHT;
                        let w = (length_steps as f32).max(1.0) * STEP_WIDTH;
                        let note_rect =
                            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, KEY_HEIGHT - 4.0));
                        if note_rect.contains(pos) {
                            let _ = view.command_tx.send(Command::PianoRollDeleteNotes {
                                channel_id: view.channel_id,
                                pattern_id: view.pattern_id,
                                note_ids: vec![note.id],
                            });
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn velocity_lane(ui: &mut egui::Ui, view: &mut PianoRollView<'_>) {
    ui.group(|ui| {
        ui.label("Velocity");
        if let Some(note_id) = view.state.selected_note {
            if let Some(clip) = view.session.clip(view.channel_id, view.pattern_id) {
                if let Some(note) = clip.notes.get(&note_id) {
                    let mut velocity = note.velocity as f32;
                    if ui
                        .add(egui::Slider::new(&mut velocity, 1.0..=127.0).text("Velocity"))
                        .changed()
                    {
                        let _ = view.command_tx.send(Command::PianoRollSetNoteVelocity {
                            channel_id: view.channel_id,
                            pattern_id: view.pattern_id,
                            note_id,
                            velocity: velocity as u8,
                        });
                    }
                }
            }
        } else {
            ui.label("Select a note to edit velocity.");
        }
    });
}

fn drag_rect(rect: Rect, start: Pos2, end: Pos2) -> Rect {
    let start = clamp_to_rect(rect, start);
    let end = clamp_to_rect(rect, end);
    Rect::from_two_pos(start, end)
}

fn drag_to_note(rect: Rect, start: Pos2, end: Pos2, ticks_per_step: u32) -> Option<(u32, u32, i8)> {
    let start = clamp_to_rect(rect, start);
    let end = clamp_to_rect(rect, end);
    if (end - start).length_sq() < 2.0 {
        return None;
    }

    let min_x = start.x.min(end.x);
    let max_x = start.x.max(end.x);
    let min_y = start.y.min(end.y);

    let start_step = ((min_x - rect.min.x) / STEP_WIDTH).floor().max(0.0) as u32;
    let end_step = ((max_x - rect.min.x) / STEP_WIDTH).ceil().max(1.0) as u32;
    let length_steps = end_step.saturating_sub(start_step).max(1);
    let key_row = ((min_y - rect.min.y) / KEY_HEIGHT).floor() as usize;
    let key = row_to_key(key_row)?;

    Some((
        start_step * ticks_per_step,
        length_steps * ticks_per_step,
        key,
    ))
}

fn clamp_to_rect(rect: Rect, pos: Pos2) -> Pos2 {
    Pos2::new(
        pos.x.clamp(rect.min.x, rect.max.x),
        pos.y.clamp(rect.min.y, rect.max.y),
    )
}

fn key_to_row(key: i8) -> Option<usize> {
    if key < LOW_KEY || key >= LOW_KEY + KEY_COUNT as i8 {
        None
    } else {
        let highest = LOW_KEY + KEY_COUNT as i8 - 1;
        Some((highest - key) as usize)
    }
}

fn row_to_key(row: usize) -> Option<i8> {
    if row >= KEY_COUNT {
        None
    } else {
        let highest = LOW_KEY + KEY_COUNT as i8 - 1;
        Some(highest - row as i8)
    }
}

fn is_black_key(row: usize) -> bool {
    row_to_key(row)
        .map(|key| matches!(key % 12, 1 | 3 | 6 | 8 | 10))
        .unwrap_or(false)
}
