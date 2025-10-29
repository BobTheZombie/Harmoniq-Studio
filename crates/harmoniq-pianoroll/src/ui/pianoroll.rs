use super::{grid_stroke, lane_color, ScaleGuide};
use crate::state::*;
use egui::{self, vec2, Color32, Rect, Rounding, Sense, Stroke};

pub struct PianoCtx<'a> {
    ui: &'a mut egui::Ui,
    roll_rect: Rect,
    key_rect: Rect,
    vel_rect: Rect,
    key_min: i8,
    key_max: i8,
    px_per_tick: f32,
    px_per_key: f32,
    ppq: u32,
    snap_ticks: u32,
}

fn bar_grid(ppq: u32, bars: u32) -> u32 {
    4 * ppq * bars
}

fn key_name(key: i8) -> &'static str {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    NAMES[(key.rem_euclid(12)) as usize]
}

pub fn render(ui: &mut egui::Ui, props: crate::PianoRollProps) {
    let crate::PianoRollProps {
        state,
        snap,
        ghost_clip,
        scale,
        mut on_changed,
        mut on_preview,
    } = props;

    let ppq = state.clip.ppq;
    let snap_ticks = snap.ticks(ppq).max(1);

    let available = ui.available_size();
    let key_width = 64.0;
    let velocity_height = 64.0;
    let roll_size =
        vec2(available.x - key_width, available.y - velocity_height).max(vec2(200.0, 120.0));
    let top_left = ui.min_rect().min;
    let key_rect = Rect::from_min_size(top_left, vec2(key_width, roll_size.y));
    let roll_rect = Rect::from_min_size(top_left + vec2(key_width, 0.0), roll_size);
    let vel_rect = Rect::from_min_size(
        top_left + vec2(0.0, roll_size.y),
        vec2(available.x, velocity_height),
    );

    ui.painter()
        .rect_filled(roll_rect, 6.0, ui.visuals().extreme_bg_color);
    ui.painter()
        .rect_filled(key_rect, 6.0, ui.visuals().extreme_bg_color);
    ui.painter()
        .rect_filled(vel_rect, 6.0, ui.visuals().extreme_bg_color);

    let px_per_tick = state.zoom_x.max(4.0) / (ppq as f32 / 4.0);
    let px_per_key = state.zoom_y.max(6.0);

    let key_min = 24i8;
    let key_max = 84i8;

    let mut ctx = PianoCtx {
        ui,
        roll_rect,
        key_rect,
        vel_rect,
        key_min,
        key_max,
        px_per_tick,
        px_per_key,
        ppq,
        snap_ticks,
    };

    draw_keys(&mut ctx, &scale);
    draw_grid(&mut ctx);
    if let Some(ghost) = ghost_clip {
        draw_notes(&mut ctx, ghost, true, &[]);
    }
    draw_notes(&mut ctx, &state.clip, false, &state.selection);

    if let Some(preview_cb) = on_preview.as_mut() {
        handle_mouse(&mut ctx, state, Some(preview_cb.as_mut()));
    } else {
        handle_mouse(&mut ctx, state, None);
    }
    draw_velocity_lane(&mut ctx, state);

    on_changed(&state.clip);
}

fn draw_keys(ctx: &mut PianoCtx<'_>, scale: &ScaleGuide) {
    let mut y = ctx.key_rect.top();
    for key in (ctx.key_min..=ctx.key_max).rev() {
        let rect = Rect::from_min_size(
            egui::pos2(ctx.key_rect.left(), y),
            vec2(ctx.key_rect.width(), ctx.px_per_key),
        );
        let mut color = lane_color(ctx.ui, key);
        if let Some(tonic) = match *scale {
            ScaleGuide::Major(t) => Some(t),
            ScaleGuide::Minor(t) => Some(t),
            ScaleGuide::None => None,
        } {
            if (key - tonic).rem_euclid(12) == 0 {
                color = color.gamma_multiply(1.25);
            }
        }
        ctx.ui.painter().rect_filled(rect, 2.0, color);
        if key % 12 == 0 {
            ctx.ui.painter().text(
                rect.left_top() + vec2(6.0, 2.0),
                egui::Align2::LEFT_TOP,
                format!("{}{}", key_name(key), (key / 12) - 1),
                egui::TextStyle::Small.resolve(ctx.ui.style()),
                ctx.ui.visuals().text_color(),
            );
        }
        y += ctx.px_per_key;
    }
}

fn draw_grid(ctx: &mut PianoCtx<'_>) {
    let bars = 4;
    let total_ticks = bar_grid(ctx.ppq, bars);
    let mut tick = 0u32;
    while tick <= total_ticks {
        let x = ctx.roll_rect.left() + tick as f32 * ctx.px_per_tick;
        let strong = tick % ctx.ppq == 0;
        ctx.ui.painter().line_segment(
            [
                egui::pos2(x, ctx.roll_rect.top()),
                egui::pos2(x, ctx.roll_rect.bottom()),
            ],
            grid_stroke(ctx.ui, strong),
        );
        tick += ctx.ppq / 4;
    }

    let mut y = ctx.roll_rect.top();
    for _ in (ctx.key_min..=ctx.key_max).rev() {
        ctx.ui.painter().line_segment(
            [
                egui::pos2(ctx.roll_rect.left(), y),
                egui::pos2(ctx.roll_rect.right(), y),
            ],
            Stroke::new(1.0, ctx.ui.visuals().weak_text_color()),
        );
        y += ctx.px_per_key;
    }
}

fn draw_notes(ctx: &mut PianoCtx<'_>, clip: &MidiClip, ghost: bool, selection: &[NoteId]) {
    for (_id, note) in &clip.notes {
        let x = ctx.roll_rect.left() + note.start_ticks as f32 * ctx.px_per_tick;
        let width = note.length_ticks.max(1) as f32 * ctx.px_per_tick;
        let key_idx = (ctx.key_max - note.key).max(0) as f32;
        let y = ctx.roll_rect.top() + key_idx * ctx.px_per_key;
        let rect = Rect::from_min_size(
            egui::pos2(x, y + 1.0),
            vec2(width.max(3.0), ctx.px_per_key - 2.0),
        );
        let color = if ghost {
            ctx.ui.visuals().text_color().gamma_multiply(0.2)
        } else if selection.contains(&note.id) {
            Color32::from_rgba_unmultiplied(120, 180, 255, 230)
        } else {
            ctx.ui.visuals().text_color()
        };
        ctx.ui
            .painter()
            .rect_filled(rect, Rounding::same(3.0), color);
        if !ghost {
            ctx.ui.painter().rect_filled(
                Rect::from_min_size(rect.left_top(), vec2(3.0, rect.height())),
                Rounding::same(2.0),
                ctx.ui.visuals().weak_text_color(),
            );
            ctx.ui.painter().rect_filled(
                Rect::from_min_size(
                    egui::pos2(rect.right() - 3.0, rect.top()),
                    vec2(3.0, rect.height()),
                ),
                Rounding::same(2.0),
                ctx.ui.visuals().weak_text_color(),
            );
        }
    }
}

fn screen_to_note(ctx: &PianoCtx<'_>, pos: egui::Pos2, snap: u32) -> (u32, i8) {
    let x_ticks = ((pos.x - ctx.roll_rect.left()) / ctx.px_per_tick).max(0.0) as u32;
    let snapped = (x_ticks / snap) * snap;
    let key_index = ((pos.y - ctx.roll_rect.top()) / ctx.px_per_key).floor() as i32;
    let key = ctx.key_max as i32 - key_index;
    (snapped, key.clamp(0, 127) as i8)
}

fn handle_mouse(
    ctx: &mut PianoCtx<'_>,
    state: &mut PianoRollState,
    mut preview: Option<&mut dyn FnMut(i8, u8)>,
) {
    let rect = ctx.roll_rect;
    let response = ctx
        .ui
        .interact(rect, ctx.ui.id().with("pr_area"), Sense::click_and_drag());
    let mut changed = false;

    if response.hovered() {
        let zoom = ctx.ui.input(|i| i.smooth_scroll_delta.y);
        if zoom.abs() > 0.0 {
            state.zoom_x = (state.zoom_x + zoom.signum() * 2.0).clamp(8.0, 64.0);
        }
    }

    match state.tool {
        Tool::Draw => {
            if response.clicked() || response.dragged() {
                if let Some(pointer) = ctx.ui.input(|i| i.pointer.interact_pos()) {
                    let (start, key) = screen_to_note(ctx, pointer, ctx.snap_ticks);
                    let length = ctx.snap_ticks.max(1);
                    let velocity = 100;
                    let id = state.clip.insert(MidiNote {
                        id: 0,
                        start_ticks: start,
                        length_ticks: length,
                        key,
                        velocity,
                    });
                    state.selection.clear();
                    state.selection.push(id);
                    if let Some(cb) = preview.as_deref_mut() {
                        cb(key, velocity as u8);
                    }
                    changed = true;
                }
            }
            if response.secondary_clicked() {
                if let Some(pointer) = ctx.ui.input(|i| i.pointer.interact_pos()) {
                    let (tick, key) = screen_to_note(ctx, pointer, 1);
                    if let Some((id, _)) = state.clip.notes.iter().find(|(_, note)| {
                        note.key == key
                            && tick >= note.start_ticks
                            && tick < note.start_ticks + note.length_ticks
                    }) {
                        let id = *id;
                        state.clip.notes.remove(&id);
                        changed = true;
                    }
                }
            }
        }
        Tool::Select => {
            if response.drag_started() {
                state.selection.clear();
            }
            if let Some(pointer) = ctx.ui.input(|i| i.pointer.interact_pos()) {
                if ctx.ui.input(|i| i.pointer.primary_down()) {
                    let (tick, key) = screen_to_note(ctx, pointer, 1);
                    for (id, note) in &state.clip.notes {
                        if note.key == key
                            && tick >= note.start_ticks
                            && tick < note.start_ticks + note.length_ticks
                            && !state.selection.contains(id)
                        {
                            state.selection.push(*id);
                        }
                    }
                }
            }
            if ctx
                .ui
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                let ids = state.selection.clone();
                state.clip.remove_many(&ids);
                state.selection.clear();
                changed = true;
            }
        }
        Tool::Slice => {
            if response.clicked() {
                if let Some(pointer) = ctx.ui.input(|i| i.pointer.interact_pos()) {
                    let (tick, key) = screen_to_note(ctx, pointer, ctx.snap_ticks);
                    if let Some((id, note)) =
                        state.clip.notes.clone().into_iter().find(|(_, note)| {
                            note.key == key
                                && tick > note.start_ticks
                                && tick < note.start_ticks + note.length_ticks
                        })
                    {
                        let right_len = note.start_ticks + note.length_ticks - tick;
                        let left_len = tick - note.start_ticks;
                        if let Some(existing) = state.clip.notes.get_mut(&id) {
                            existing.length_ticks = left_len;
                        }
                        state.clip.insert(MidiNote {
                            id: 0,
                            start_ticks: tick,
                            length_ticks: right_len,
                            key,
                            velocity: note.velocity,
                        });
                        changed = true;
                    }
                }
            }
        }
    }

    if !state.selection.is_empty() {
        let selected_ids = state.selection.clone();
        if response.dragged() {
            if let Some(_pointer) = ctx.ui.input(|i| i.pointer.interact_pos()) {
                let delta = ctx.ui.input(|i| i.pointer.delta());
                let delta_ticks = (delta.x / ctx.px_per_tick).round() as i32;
                let delta_keys = (delta.y / ctx.px_per_key).round() as i32;
                if delta_ticks != 0 || delta_keys != 0 {
                    for id in &selected_ids {
                        if let Some(note) = state.clip.notes.get_mut(id) {
                            let mut start = (note.start_ticks as i32
                                + delta_ticks * ctx.snap_ticks as i32)
                                .max(0) as u32;
                            start = (start / ctx.snap_ticks) * ctx.snap_ticks;
                            let key = (note.key as i32 - delta_keys).clamp(0, 127) as i8;
                            note.start_ticks = start;
                            note.key = key;
                        }
                    }
                    changed = true;
                }
            }
        }
        if ctx
            .ui
            .input(|i| i.modifiers.shift && i.pointer.primary_down())
        {
            if ctx.ui.input(|i| i.pointer.delta().x.abs() > 0.0) {
                let delta = ctx.ui.input(|i| i.pointer.delta().x);
                let delta_len = (delta / ctx.px_per_tick).round() as i32 * ctx.snap_ticks as i32;
                if delta_len != 0 {
                    for id in &selected_ids {
                        if let Some(note) = state.clip.notes.get_mut(id) {
                            let length = (note.length_ticks as i32 + delta_len)
                                .max(ctx.snap_ticks as i32)
                                as u32;
                            note.length_ticks = length;
                        }
                    }
                    changed = true;
                }
            }
        }
        if ctx
            .ui
            .input(|i| i.modifiers.command_only() && i.key_pressed(egui::Key::D))
        {
            let snapshot: Vec<_> = selected_ids
                .iter()
                .filter_map(|id| state.clip.notes.get(id).cloned())
                .collect();
            for mut note in snapshot {
                note.start_ticks += note.length_ticks;
                note.id = 0;
                state.clip.insert(note);
            }
            changed = true;
        }
    }

    if changed {
        ctx.ui.ctx().request_repaint();
    }
}

fn draw_velocity_lane(ctx: &mut PianoCtx<'_>, state: &mut PianoRollState) {
    let rect = ctx.vel_rect;
    ctx.ui.painter().text(
        rect.left_top() + vec2(8.0, 4.0),
        egui::Align2::LEFT_TOP,
        "Velocity",
        egui::TextStyle::Small.resolve(ctx.ui.style()),
        ctx.ui.visuals().text_color(),
    );
    let items: Vec<_> = if state.selection.is_empty() {
        state.clip.notes.values().cloned().collect()
    } else {
        state
            .selection
            .iter()
            .filter_map(|id| state.clip.notes.get(id).cloned())
            .collect()
    };
    for note in items {
        let x = ctx.roll_rect.left() + note.start_ticks as f32 * ctx.px_per_tick;
        let width = note.length_ticks.max(1) as f32 * ctx.px_per_tick;
        let height = (note.velocity as f32 / 127.0) * (ctx.vel_rect.height() - 18.0);
        let bar = Rect::from_min_size(
            egui::pos2(x, ctx.vel_rect.bottom() - height - 6.0),
            vec2(width.max(4.0), height),
        );
        ctx.ui
            .painter()
            .rect_filled(bar, Rounding::same(2.0), ctx.ui.visuals().text_color());
    }
    let response = ctx.ui.interact(
        rect,
        ctx.ui.id().with("vel_area"),
        egui::Sense::click_and_drag(),
    );
    if response.hovered()
        && ctx
            .ui
            .input(|i| i.modifiers.alt && i.pointer.primary_down())
    {
        if let Some(pointer) = ctx.ui.input(|i| i.pointer.interact_pos()) {
            let (tick, _) = screen_to_note(ctx, pointer, 1);
            let value = ((ctx.vel_rect.bottom() - pointer.y - 6.0) / (ctx.vel_rect.height() - 18.0)
                * 127.0)
                .clamp(1.0, 127.0) as u8;
            let target = if let Some(id) = state.selection.first() {
                state.clip.notes.get_mut(id)
            } else {
                state.clip.notes.iter_mut().find_map(|(_, note)| {
                    if tick >= note.start_ticks && tick < note.start_ticks + note.length_ticks {
                        Some(note)
                    } else {
                        None
                    }
                })
            };
            if let Some(note) = target {
                note.velocity = value;
                ctx.ui.ctx().request_repaint();
            }
        }
    }
}
