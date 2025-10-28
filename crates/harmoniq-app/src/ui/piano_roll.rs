use crossbeam_channel::Sender;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense};

use crate::commands::Command;
use crate::state::session::{ChannelId, MidiNote, NoteId, PatternId};

const TOTAL_STEPS: u32 = 16;
const MIN_KEY: i32 = 48; // C3
const MAX_KEY: i32 = 72; // C5

#[derive(Default)]
pub struct PianoRollState {
    drag: Option<DragInfo>,
    pub velocity: u8,
}

#[derive(Clone, Copy)]
struct DragInfo {
    start_tick: u32,
    current_tick: u32,
    key: i8,
}

impl Default for DragInfo {
    fn default() -> Self {
        Self {
            start_tick: 0,
            current_tick: 0,
            key: 60,
        }
    }
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            drag: None,
            velocity: 96,
        }
    }
}

pub struct PianoRollContext<'a> {
    pub channel_id: ChannelId,
    pub pattern_id: PatternId,
    pub channel_name: &'a str,
    pub pattern_name: &'a str,
    pub ppq: u32,
    pub notes: &'a [(NoteId, MidiNote)],
}

pub fn render(
    ui: &mut egui::Ui,
    state: &mut PianoRollState,
    ctx: &PianoRollContext<'_>,
    tx: &Sender<Command>,
) {
    header(ui, ctx, state, tx);
    ui.add_space(8.0);
    canvas(ui, state, ctx, tx);
}

fn header(
    ui: &mut egui::Ui,
    ctx: &PianoRollContext<'_>,
    state: &mut PianoRollState,
    tx: &Sender<Command>,
) {
    ui.horizontal(|ui| {
        let title = format!("Piano Roll – {}, {}", ctx.channel_name, ctx.pattern_name);
        ui.label(RichText::new(title).heading());
        ui.add_space(12.0);
        ui.label("Velocity");
        let mut velocity = state.velocity as f32;
        if ui
            .add_sized([120.0, 16.0], egui::Slider::new(&mut velocity, 1.0..=127.0))
            .changed()
        {
            state.velocity = velocity.round() as u8;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                let _ = tx.send(Command::ClosePianoRoll);
            }
        });
    });
}

fn canvas(
    ui: &mut egui::Ui,
    state: &mut PianoRollState,
    ctx: &PianoRollContext<'_>,
    tx: &Sender<Command>,
) {
    let ticks_per_step = (ctx.ppq / 4).max(1);
    let rows = (MAX_KEY - MIN_KEY + 1) as usize;
    let key_height = 22.0;
    let step_width = 32.0;
    let total_size = egui::vec2(step_width * TOTAL_STEPS as f32, key_height * rows as f32);

    let (response, painter) = ui.allocate_painter(total_size, Sense::click_and_drag());
    let rect = response.rect;

    draw_grid(&painter, rect, rows, key_height, step_width);
    let mut note_rects: Vec<(NoteId, Rect)> = Vec::new();

    for (note_id, note) in ctx.notes.iter() {
        let note_rect = note_rect(rect, note, ticks_per_step, key_height, step_width);
        note_rects.push((*note_id, note_rect));
        paint_note(&painter, note_rect, Color32::from_rgb(130, 190, 255));
    }

    if let Some(drag) = &mut state.drag {
        if let Some(pos) = response.interact_pointer_pos() {
            drag.current_tick = snap_tick(rect, pos, ticks_per_step, step_width)
                .max(drag.start_tick + ticks_per_step);
            drag.key = snap_key(rect, pos, key_height);
        }

        let preview_note = MidiNote {
            id: 0,
            start_ticks: drag.start_tick,
            length_ticks: drag.current_tick.saturating_sub(drag.start_tick),
            key: drag.key,
            velocity: state.velocity,
        };
        let preview_rect = note_rect(rect, &preview_note, ticks_per_step, key_height, step_width);
        paint_note(
            &painter,
            preview_rect,
            Color32::from_rgba_unmultiplied(255, 140, 100, 120),
        );
    }

    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            if rect.contains(pos) {
                let start_tick = snap_tick(rect, pos, ticks_per_step, step_width);
                let key = snap_key(rect, pos, key_height);
                state.drag = Some(DragInfo {
                    start_tick,
                    current_tick: start_tick + ticks_per_step,
                    key,
                });
            }
        }
    }

    if response.drag_released() {
        if let Some(drag) = state.drag.take() {
            let length = drag
                .current_tick
                .saturating_sub(drag.start_tick)
                .max(ticks_per_step);
            let _ = tx.send(Command::PianoRollInsertNotes {
                channel_id: ctx.channel_id,
                pattern_id: ctx.pattern_id,
                notes: vec![(drag.start_tick, length, drag.key, state.velocity)],
            });
        }
    }

    if response.clicked() && state.drag.is_none() {
        if let Some(pos) = response.interact_pointer_pos() {
            if rect.contains(pos) {
                let start_tick = snap_tick(rect, pos, ticks_per_step, step_width);
                let key = snap_key(rect, pos, key_height);
                let _ = tx.send(Command::PianoRollInsertNotes {
                    channel_id: ctx.channel_id,
                    pattern_id: ctx.pattern_id,
                    notes: vec![(start_tick, ticks_per_step, key, state.velocity)],
                });
            }
        }
    }

    if response.secondary_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((note_id, _)) = note_rects
                .iter()
                .find(|(_, rect)| rect.contains(pos))
                .cloned()
            {
                let _ = tx.send(Command::PianoRollDeleteNotes {
                    channel_id: ctx.channel_id,
                    pattern_id: ctx.pattern_id,
                    note_ids: vec![note_id],
                });
            }
        }
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, rows: usize, key_height: f32, step_width: f32) {
    painter.rect_filled(rect, 6.0, Color32::from_gray(28));

    for row in 0..rows {
        let y = rect.top() + row as f32 * key_height;
        let key = MAX_KEY - row as i32;
        let is_black = is_black_key(key);
        let fill = if is_black {
            Color32::from_gray(22)
        } else {
            Color32::from_gray(32)
        };
        let row_rect = Rect::from_min_size(
            Pos2::new(rect.left(), y),
            egui::vec2(rect.width(), key_height),
        );
        painter.rect_filled(row_rect, 0.0, fill);
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            egui::Stroke::new(0.5, Color32::from_gray(60)),
        );
    }

    for step in 0..=TOTAL_STEPS {
        let x = rect.left() + step as f32 * step_width;
        let strong = step % 4 == 0;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            egui::Stroke::new(if strong { 1.2 } else { 0.5 }, Color32::from_gray(80)),
        );
    }
}

fn note_rect(
    rect: Rect,
    note: &MidiNote,
    ticks_per_step: u32,
    key_height: f32,
    step_width: f32,
) -> Rect {
    let start_steps = note.start_ticks as f32 / ticks_per_step as f32;
    let length_steps = (note.length_ticks as f32 / ticks_per_step as f32).max(1.0);
    let x = rect.left() + start_steps * step_width;
    let width = length_steps * step_width;
    let row = (MAX_KEY - note.key as i32).clamp(0, MAX_KEY - MIN_KEY) as f32;
    let y = rect.top() + row * key_height;
    Rect::from_min_size(Pos2::new(x, y + 2.0), egui::vec2(width, key_height - 4.0))
}

fn paint_note(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect, 4.0, color);
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(20)));
}

fn snap_tick(rect: Rect, pos: Pos2, ticks_per_step: u32, step_width: f32) -> u32 {
    let relative = (pos.x - rect.left()).clamp(0.0, rect.width());
    let step = (relative / step_width).floor() as u32;
    step.min(TOTAL_STEPS - 1) * ticks_per_step
}

fn snap_key(rect: Rect, pos: Pos2, key_height: f32) -> i8 {
    let relative = (pos.y - rect.top()).clamp(0.0, rect.height());
    let row = (relative / key_height).floor() as i32;
    let key = MAX_KEY - row;
    key.clamp(MIN_KEY, MAX_KEY) as i8
}

fn is_black_key(key: i32) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10 | -11 | -9 | -6 | -4 | -1)
}
