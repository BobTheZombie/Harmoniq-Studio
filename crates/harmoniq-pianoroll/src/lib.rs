//! Piano roll editor for Harmoniq Studio.
//!
//! The piano roll is implemented as an [`egui`] widget with a dense layout and
//! a data model designed for real-time audio environments. The entry point is
//! [`PianoRoll`], a stateful component that owns the editor state and exposes a
//! [`PianoRoll::ui`] function for integration in host applications.

pub mod controller_lanes;
pub mod model;
pub mod theme;
pub mod tools;
pub mod transport;

pub use tools::NotePreview;

use std::ops::RangeInclusive;

use controller_lanes::lanes_ui;
use egui::{
    pos2, vec2, Align2, Color32, Layout, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui,
};
use model::{Clip, Edit, EditorState, Note, QuantizePreset, SnapUnit};
use theme::{Spacing, Theme};
use tools::{HitNote, PointerPosition, Tool, ToolController, ToolOutput};
use transport::ruler_ui;

/// Stateful piano roll widget. The widget owns the `EditorState` so it can
/// manage undo/redo and emit changes back to the engine in batches.
pub struct PianoRoll {
    state: EditorState,
    theme: Theme,
    spacing: Spacing,
    tool_controller: ToolController,
    note_shapes: Vec<Shape>,
    ghost_shapes: Vec<Shape>,
    grid_shapes: Vec<Shape>,
    pending_edits: Vec<Edit>,
    pending_previews: Vec<NotePreview>,
    hovered_note: Option<u64>,
    marquee_rect: Option<Rect>,
    gesture_edits: Vec<Edit>,
    history_snapshot: Option<Clip>,
    history_dirty: bool,
}

impl PianoRoll {
    /// Creates a new piano roll from the provided [`EditorState`].
    pub fn new(state: EditorState) -> Self {
        let ppq = state.ppq();
        let snap = state.snap;
        let triplets = state.triplets;
        Self {
            state,
            theme: Theme::default(),
            spacing: Spacing::compact(),
            tool_controller: ToolController::new(ppq, snap, triplets),
            note_shapes: Vec::new(),
            ghost_shapes: Vec::new(),
            grid_shapes: Vec::new(),
            pending_edits: Vec::new(),
            pending_previews: Vec::new(),
            hovered_note: None,
            marquee_rect: None,
            gesture_edits: Vec::new(),
            history_snapshot: None,
            history_dirty: false,
        }
    }

    /// Returns the current editor state.
    pub fn state(&self) -> &EditorState {
        &self.state
    }

    /// Returns a mutable reference to the editor state.
    pub fn state_mut(&mut self) -> &mut EditorState {
        &mut self.state
    }

    /// Provides mutable access to the theme so hosts can customise the look.
    pub fn theme_mut(&mut self) -> &mut Theme {
        &mut self.theme
    }

    /// Replace the currently edited clip.
    pub fn set_clip(&mut self, clip: Clip) {
        self.state.clip = clip;
        self.state.clip.sort_notes();
        self.tool_controller.update_snapper(
            self.state.ppq(),
            self.state.snap,
            self.state.triplets,
            self.state.quantize_swing,
        );
    }

    /// Drains the edits accumulated during the previous call to [`PianoRoll::ui`].
    pub fn take_edits(&mut self) -> Vec<Edit> {
        self.pending_edits.drain(..).collect()
    }

    /// Drains the accumulated note preview requests.
    pub fn take_note_previews(&mut self) -> Vec<NotePreview> {
        self.pending_previews.drain(..).collect()
    }

    /// Renders the piano roll inside the provided `egui::Ui`.
    pub fn ui(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        self.tool_controller.update_snapper(
            self.state.ppq(),
            self.state.snap,
            self.state.triplets,
            self.state.quantize_swing,
        );

        self.top_toolbar(ui);

        let ruler = ruler_ui(ui, &mut self.state, &self.theme, width);
        self.pending_edits.extend(ruler.edits);

        let lanes_height: f32 = self
            .state
            .lanes
            .iter()
            .filter(|lane| lane.visible)
            .map(|lane| lane.height + self.spacing.lane_spacing)
            .sum();
        let available_height = ui.available_height().max(120.0);
        let main_height = (available_height - lanes_height).max(160.0);

        let (rect, response) =
            ui.allocate_exact_size(vec2(width, main_height), Sense::click_and_drag());
        let keyboard_rect =
            Rect::from_min_size(rect.min, vec2(self.spacing.keyboard_width, rect.height()));
        let grid_rect =
            Rect::from_min_max(pos2(keyboard_rect.right(), rect.top()), rect.right_bottom());

        self.handle_input(ui, keyboard_rect, grid_rect, &response);
        self.paint_keyboard(ui.painter_at(keyboard_rect), keyboard_rect);
        self.paint_grid(ui.painter_at(grid_rect), grid_rect);
        self.paint_notes(ui.painter_at(grid_rect), grid_rect);
        if let Some(marquee) = self.marquee_rect {
            let painter = ui.painter();
            painter.rect_filled(marquee, 0.0, self.theme.selection_rect);
            painter.rect_stroke(marquee, 0.0, self.theme.selection_rect_border);
        }

        if lanes_height > 0.0 {
            ui.allocate_ui_with_layout(
                vec2(width, lanes_height),
                Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let result = lanes_ui(ui, &mut self.state, &self.theme, width);
                    self.pending_edits.extend(result.edits);
                },
            );
        }
    }

    fn top_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let tool_buttons = [
                ("Select", Tool::Arrow),
                ("Draw", Tool::Draw),
                ("Erase", Tool::Erase),
                ("Split", Tool::Split),
                ("Glue", Tool::Glue),
            ];
            for (label, tool) in tool_buttons {
                let selected = self.state.tool == tool;
                if ui.selectable_label(selected, label).clicked() {
                    self.state.tool = tool;
                    self.tool_controller.set_tool(tool);
                }
            }
            ui.separator();
            egui::ComboBox::from_label("Snap")
                .selected_text(self.snap_label())
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.state.snap.is_none(), "Off")
                        .clicked()
                    {
                        self.state.snap = None;
                    }
                    for (name, unit) in [("Bar", SnapUnit::Bar), ("Beat", SnapUnit::Beat)] {
                        if ui
                            .selectable_label(self.state.snap == Some(unit), name)
                            .clicked()
                        {
                            self.state.snap = Some(unit);
                        }
                    }
                    ui.separator();
                    for div in [1, 2, 3, 4, 6, 8, 12, 16, 24, 32] {
                        let unit = SnapUnit::Grid(div);
                        if ui
                            .selectable_label(self.state.snap == Some(unit), format!("1/{}", div))
                            .clicked()
                        {
                            self.state.snap = Some(unit);
                        }
                    }
                });
            ui.toggle_value(&mut self.state.triplets, "Triplet");
            ui.toggle_value(&mut self.state.follow_playhead, "Follow");
            ui.separator();
            let mut loop_beats = self.state.clip.loop_len_ppq as f32 / self.state.ppq() as f32;
            let len_response = ui
                .add(
                    egui::DragValue::new(&mut loop_beats)
                        .clamp_range(1.0..=512.0)
                        .speed(0.25)
                        .suffix(" beats"),
                )
                .on_hover_text("Pattern length");
            if len_response.changed() {
                let new_len = (loop_beats * self.state.ppq() as f32).round().max(1.0) as i64;
                self.begin_history_snapshot();
                self.state.clip.loop_len_ppq = new_len;
                self.history_dirty = true;
                let edit = Edit::LoopChanged {
                    start_ppq: self.state.clip.loop_start_ppq,
                    len_ppq: self.state.clip.loop_len_ppq,
                };
                self.pending_edits.push(edit.clone());
                self.gesture_edits.push(edit);
                self.commit_history_snapshot();
                self.gesture_edits.clear();
            }
            ui.separator();
            ui.label("Quantize");
            ui.add(
                egui::Slider::new(&mut self.state.quantize_strength, 0.0..=1.0).text("Strength"),
            );
            ui.add(egui::Slider::new(&mut self.state.quantize_swing, -0.75..=0.75).text("Swing"));
            if ui.button("Apply Q").clicked() {
                if let Some(snap) = self.state.snap.or(Some(SnapUnit::Grid(4))) {
                    let preset = QuantizePreset {
                        name: "Grid".to_owned(),
                        snap,
                        strength: self.state.quantize_strength,
                        swing: self.state.quantize_swing,
                        range: RangeInclusive::new(i64::MIN, i64::MAX),
                        iterative: false,
                    };
                    self.pending_edits.push(Edit::Quantize {
                        preset,
                        strength: self.state.quantize_strength,
                        swing: self.state.quantize_swing,
                    });
                }
            }
        });
    }

    fn handle_input(&mut self, ui: &Ui, keyboard_rect: Rect, grid_rect: Rect, response: &Response) {
        self.handle_scroll_and_zoom(ui, grid_rect);
        let modifiers = ui.ctx().input(|i| i.modifiers);
        if response.clicked() {
            response.request_focus();
        }
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                if keyboard_rect.contains(pos) {
                    let pitch = tools::pointer_to_pitch(
                        self.state.zoom_y,
                        self.state.scroll_px.y,
                        keyboard_rect,
                        pos.y,
                    );
                    let channel = self
                        .state
                        .clip
                        .notes
                        .iter()
                        .find(|note| note.selected)
                        .map(|note| note.chan)
                        .unwrap_or(0);
                    self.pending_previews.push(NotePreview {
                        pitch,
                        velocity: 100,
                        channel,
                    });
                    return;
                }
                self.begin_history_snapshot();
                self.gesture_edits.clear();
                let pointer = self.pointer_position(grid_rect, pos);
                let hit = self.hit_test(grid_rect, pos);
                let output = self.tool_controller.on_pointer_pressed(
                    &mut self.state,
                    pointer,
                    hit,
                    modifiers,
                );
                self.handle_tool_output(output);
            }
        }
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let pointer = self.pointer_position(grid_rect, pos);
                let output =
                    self.tool_controller
                        .on_pointer_dragged(&mut self.state, pointer, modifiers);
                self.handle_tool_output(output);
            }
        }
        if response.clicked_by(egui::PointerButton::Secondary) {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(hit) = self.hit_test(grid_rect, pos) {
                    self.begin_history_snapshot();
                    if self.state.remove_note(hit.id).is_some() {
                        self.pending_edits.push(Edit::Remove(hit.id));
                        self.gesture_edits.push(Edit::Remove(hit.id));
                        self.history_dirty = true;
                    }
                }
            }
        }
        if response.drag_stopped() || response.clicked_elsewhere() {
            self.finalize_marquee(grid_rect);
            self.tool_controller.on_pointer_released();
            if !self.gesture_edits.is_empty() {
                self.commit_history_snapshot();
                self.gesture_edits.clear();
            } else {
                self.history_snapshot = None;
                self.history_dirty = false;
            }
        }
        if response.hovered() {
            if let Some(pos) = response.hover_pos() {
                self.hovered_note = self.hit_test(grid_rect, pos).map(|hit| hit.id);
            }
        }

        self.handle_keyboard(response, grid_rect);
    }

    fn handle_scroll_and_zoom(&mut self, ui: &Ui, grid_rect: Rect) {
        ui.input(|input| {
            let scroll = input.smooth_scroll_delta;
            if scroll == egui::Vec2::ZERO {
                return;
            }
            if input.modifiers.ctrl || input.modifiers.command {
                let zoom_factor = (1.0_f32 + scroll.y * 0.01).clamp(0.5, 2.0);
                self.state.zoom_x = (self.state.zoom_x * zoom_factor).clamp(12.0, 480.0);
            } else if input.modifiers.alt {
                let zoom_factor = (1.0_f32 + scroll.y * 0.01).clamp(0.5, 2.0);
                self.state.zoom_y =
                    (self.state.zoom_y * zoom_factor).clamp(self.spacing.row_height_min, 72.0);
            } else {
                self.state.scroll_px.x = (self.state.scroll_px.x - scroll.x).max(0.0);
                self.state.scroll_px.y += scroll.y;
            }
        });
        if self.state.scroll_px.y < -grid_rect.height() {
            self.state.scroll_px.y = -grid_rect.height();
        }
    }

    fn handle_tool_output(&mut self, output: ToolOutput) {
        if !output.edits.is_empty() {
            self.pending_edits.extend(output.edits.clone());
            self.gesture_edits.extend(output.edits);
            self.history_dirty = true;
        }
        if let Some(selection) = output.selection {
            self.state.clear_selection();
            for id in selection {
                self.state.select_note(id, true);
            }
        }
        if let Some(preview) = output.preview {
            self.pending_previews.push(preview);
        }
        if let Some(rect) = output.marquee {
            self.marquee_rect = Some(rect);
        } else if self.marquee_rect.is_some() {
            // Keep existing marquee until released.
        }
        if let Some(pan) = output.request_pan {
            self.state.scroll_px = pan;
        }
    }

    fn finalize_marquee(&mut self, grid_rect: Rect) {
        if let Some(rect) = self.marquee_rect.take() {
            let mut selection = Vec::new();
            for note in &self.state.clip.notes {
                let note_rect = self.note_rect(note, grid_rect);
                if rect.intersects(note_rect) {
                    selection.push(note.id);
                }
            }
            self.state.clear_selection();
            for id in selection {
                self.state.select_note(id, true);
            }
        }
    }

    fn begin_history_snapshot(&mut self) {
        if self.history_snapshot.is_none() {
            self.history_snapshot = Some(self.state.clip.clone());
        }
    }

    fn commit_history_snapshot(&mut self) {
        if self.history_dirty {
            if let Some(snapshot) = self.history_snapshot.take() {
                self.state.register_history_snapshot(snapshot);
            }
        }
        self.history_dirty = false;
    }

    fn handle_keyboard(&mut self, response: &Response, _grid_rect: Rect) {
        if !response.has_focus() {
            return;
        }

        let mut left = false;
        let mut right = false;
        let mut up = false;
        let mut down = false;
        let mut delete = false;
        let mut undo = false;
        let mut redo = false;
        let mut modifiers = egui::Modifiers::default();

        response.ctx.input(|input| {
            modifiers = input.modifiers;
            left = input.key_pressed(egui::Key::ArrowLeft);
            right = input.key_pressed(egui::Key::ArrowRight);
            up = input.key_pressed(egui::Key::ArrowUp);
            down = input.key_pressed(egui::Key::ArrowDown);
            delete =
                input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace);
            undo = input.key_pressed(egui::Key::Z)
                && (input.modifiers.ctrl || input.modifiers.command);
            redo = input.key_pressed(egui::Key::Y)
                || (input.key_pressed(egui::Key::Z)
                    && (input.modifiers.ctrl || input.modifiers.command)
                    && input.modifiers.shift);
        });

        if undo {
            let before = self.state.clip.clone();
            if self.state.undo() {
                let after = self.state.clip.clone();
                self.emit_clip_diff(before, after);
            }
        } else if redo {
            let before = self.state.clip.clone();
            if self.state.redo() {
                let after = self.state.clip.clone();
                self.emit_clip_diff(before, after);
            }
        }

        let mut edits = Vec::new();
        if left || right || up || down {
            let step = if modifiers.alt {
                1
            } else {
                self.tool_controller.snapper.step_ppq()
            };
            let mut moved = false;
            for note in &mut self.state.clip.notes {
                if !note.selected {
                    continue;
                }
                moved = true;
                if left {
                    note.start_ppq = (note.start_ppq - step).max(0);
                }
                if right {
                    note.start_ppq = (note.start_ppq + step).max(0);
                }
                if up {
                    note.pitch = note.pitch.saturating_add(1).min(127);
                }
                if down {
                    note.pitch = note.pitch.saturating_sub(1);
                }
                edits.push(Edit::Update {
                    id: note.id,
                    start_ppq: note.start_ppq,
                    dur_ppq: note.dur_ppq,
                    pitch: note.pitch,
                    vel: note.vel,
                    chan: note.chan,
                });
            }
            if moved {
                self.state.clip.sort_notes();
                self.begin_history_snapshot();
                self.history_dirty = true;
            }
        }

        if delete {
            let selection: Vec<u64> = self.state.selection.clone();
            if !selection.is_empty() {
                self.begin_history_snapshot();
                for id in selection {
                    if self.state.remove_note(id).is_some() {
                        edits.push(Edit::Remove(id));
                    }
                }
                self.history_dirty = true;
            }
        }

        if !edits.is_empty() {
            self.pending_edits.extend(edits.clone());
            self.gesture_edits.extend(edits);
            self.commit_history_snapshot();
            self.gesture_edits.clear();
        }
    }

    fn emit_clip_diff(&mut self, before: Clip, after: Clip) {
        let mut edits = Vec::new();
        for note in before.notes {
            edits.push(Edit::Remove(note.id));
        }
        for note in &after.notes {
            edits.push(Edit::Add(note.clone()));
        }
        if before.loop_start_ppq != after.loop_start_ppq
            || before.loop_len_ppq != after.loop_len_ppq
        {
            edits.push(Edit::LoopChanged {
                start_ppq: after.loop_start_ppq,
                len_ppq: after.loop_len_ppq,
            });
        }
        self.pending_edits.extend(edits.clone());
        self.gesture_edits.extend(edits);
    }

    fn pointer_position(&self, grid_rect: Rect, pos: Pos2) -> PointerPosition {
        let time_ppq = tools::pointer_to_ppq(
            &self.state.clip,
            self.state.zoom_x,
            self.state.scroll_px.x,
            pos.x - grid_rect.left(),
        );
        let pitch =
            tools::pointer_to_pitch(self.state.zoom_y, self.state.scroll_px.y, grid_rect, pos.y);
        PointerPosition {
            pos,
            time_ppq,
            pitch,
        }
    }

    fn hit_test(&self, grid_rect: Rect, pos: Pos2) -> Option<HitNote> {
        let mut best = None;
        let mut best_distance = f32::MAX;
        for note in &self.state.clip.notes {
            let rect = self.note_rect(note, grid_rect);
            if !rect.contains(pos) {
                continue;
            }
            let distance = (rect.center().x - pos.x).abs();
            if distance < best_distance {
                best_distance = distance;
                best = Some(HitNote { id: note.id, rect });
            }
        }
        best
    }

    fn paint_grid(&mut self, painter: Painter, rect: Rect) {
        self.grid_shapes.clear();
        self.paint_pitch_rows(rect);
        let beats_per_bar = self.state.beats_per_bar();
        let start_beats = (self.state.scroll_px.x / self.state.zoom_x)
            .floor()
            .max(0.0);
        let end_beats = start_beats + rect.width() / self.state.zoom_x + 4.0;
        let start_bar = (start_beats / beats_per_bar as f32).floor() as i32;
        let end_bar = (end_beats / beats_per_bar as f32).ceil() as i32;
        let ppq = self.state.ppq();
        for bar in start_bar..=end_bar {
            let bar_ppq = bar as i64 * beats_per_bar as i64 * ppq as i64;
            let x = self.time_to_x(rect, bar_ppq);
            if x < rect.left() || x > rect.right() {
                continue;
            }
            self.grid_shapes.push(Shape::line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                self.theme.grid_bar,
            ));
            for beat in 1..beats_per_bar {
                let beat_ppq = bar_ppq + beat as i64 * ppq as i64;
                let bx = self.time_to_x(rect, beat_ppq);
                if bx < rect.left() || bx > rect.right() {
                    continue;
                }
                self.grid_shapes.push(Shape::line_segment(
                    [pos2(bx, rect.top()), pos2(bx, rect.bottom())],
                    self.theme.grid_beat,
                ));
                if let Some(snap) = self.state.snap {
                    let div = snap.divisions_per_beat();
                    for sub in 1..div {
                        let sub_ppq = beat_ppq + (ppq as i64 * sub as i64) / div as i64;
                        let sx = self.time_to_x(rect, sub_ppq);
                        if sx < rect.left() || sx > rect.right() {
                            continue;
                        }
                        self.grid_shapes.push(Shape::line_segment(
                            [pos2(sx, rect.top()), pos2(sx, rect.bottom())],
                            self.theme.grid_subdivision,
                        ));
                    }
                }
            }
        }
        painter.extend(self.grid_shapes.drain(..));
    }

    fn paint_pitch_rows(&mut self, rect: Rect) {
        let min_pitch = 0;
        let max_pitch = 127;
        for pitch in min_pitch..=max_pitch {
            let top = self.pitch_to_y(rect, pitch as u8 + 1);
            let bottom = self.pitch_to_y(rect, pitch as u8);
            let row_rect = Rect::from_min_max(
                pos2(rect.left(), top.min(bottom)),
                pos2(rect.right(), top.max(bottom)),
            );
            if row_rect.max.y < rect.top() || row_rect.min.y > rect.bottom() {
                continue;
            }
            let is_black = is_black_key(pitch as u8);
            let fill = if let Some(scale) = &self.state.scale_highlight {
                if scale.contains(pitch as u8) {
                    if pitch % 12 == (self.state.key_sig.0 as i32 % 12) {
                        self.theme.grid_root_highlight
                    } else {
                        self.theme.grid_scale_highlight
                    }
                } else if is_black {
                    Color32::from_rgba_unmultiplied(24, 24, 28, 90)
                } else {
                    Color32::from_rgba_unmultiplied(16, 16, 22, 80)
                }
            } else if is_black {
                Color32::from_rgba_unmultiplied(24, 24, 28, 90)
            } else {
                Color32::from_rgba_unmultiplied(16, 16, 22, 60)
            };
            self.grid_shapes
                .push(Shape::rect_filled(row_rect, 0.0, fill));
            self.grid_shapes.push(Shape::line_segment(
                [
                    pos2(rect.left(), row_rect.max.y),
                    pos2(rect.right(), row_rect.max.y),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(40, 40, 45, 160)),
            ));
        }
    }

    fn paint_notes(&mut self, painter: Painter, rect: Rect) {
        self.note_shapes.clear();
        self.ghost_shapes.clear();
        if let Some(ghost) = &self.state.ghost_clip {
            for note in &ghost.notes {
                let note_rect = self.note_rect(note, rect);
                if note_rect.max.x < rect.left() || note_rect.min.x > rect.right() {
                    continue;
                }
                if note_rect.max.y < rect.top() || note_rect.min.y > rect.bottom() {
                    continue;
                }
                self.ghost_shapes.push(Shape::rect_filled(
                    note_rect,
                    2.5,
                    self.theme.ghost_note_fill,
                ));
                self.ghost_shapes.push(Shape::rect_stroke(
                    note_rect,
                    2.5,
                    self.theme.ghost_note_border,
                ));
            }
        }
        for note in &self.state.clip.notes {
            let note_rect = self.note_rect(note, rect);
            if note_rect.max.x < rect.left() || note_rect.min.x > rect.right() {
                continue;
            }
            if note_rect.max.y < rect.top() || note_rect.min.y > rect.bottom() {
                continue;
            }
            let base_fill = if note.selected {
                self.theme.note_selected_fill
            } else {
                self.theme.note_fill
            };
            let stroke = if note.selected {
                self.theme.note_selected_border
            } else {
                self.theme.note_border
            };
            let fill = apply_velocity_tint(base_fill, note.vel);
            self.note_shapes
                .push(Shape::rect_filled(note_rect, 3.0, fill));
            self.note_shapes
                .push(Shape::rect_stroke(note_rect, 3.0, stroke));
            let velocity_height =
                (note_rect.height() * (note.vel as f32 / 127.0)).clamp(3.0, note_rect.height());
            let vel_rect = Rect::from_min_max(
                pos2(note_rect.left() + 1.0, note_rect.bottom() - velocity_height),
                pos2(note_rect.left() + 3.0, note_rect.bottom() - 2.0),
            );
            self.note_shapes
                .push(Shape::rect_filled(vel_rect, 2.0, stroke.color));
        }
        painter.extend(self.ghost_shapes.drain(..));
        painter.extend(self.note_shapes.drain(..));
    }

    fn paint_keyboard(&mut self, painter: Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, self.theme.keyboard_background);
        let key_height = self.state.zoom_y;
        for pitch in 0..=127 {
            let y = rect.bottom() - (pitch as f32 + 1.0) * key_height + self.state.scroll_px.y;
            let key_rect =
                Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), key_height));
            if key_rect.max.y < rect.top() || key_rect.min.y > rect.bottom() {
                continue;
            }
            let is_black = is_black_key(pitch as u8);
            let fill = if is_black {
                self.theme.keyboard_black
            } else {
                self.theme.keyboard_white
            };
            painter.rect_filled(key_rect, 0.0, fill);
            painter.rect_stroke(
                key_rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(40, 40, 45, 180)),
            );
            if pitch % 12 == 0 {
                painter.text(
                    key_rect.left_top() + vec2(6.0, 2.0),
                    Align2::LEFT_TOP,
                    format!("C{}", pitch / 12 - 1),
                    egui::FontId::proportional(11.0),
                    self.theme.text,
                );
            }
        }
    }

    fn note_rect(&self, note: &Note, rect: Rect) -> Rect {
        let start_x = self.time_to_x(rect, note.start_ppq);
        let width = (note.dur_ppq as f32 / self.state.ppq() as f32 * self.state.zoom_x).max(4.0);
        let top = self.pitch_to_y(rect, note.pitch + 1);
        let height = self.state.zoom_y.max(self.spacing.row_height_min);
        Rect::from_min_size(pos2(start_x, top), vec2(width, height))
    }

    fn pitch_to_y(&self, rect: Rect, pitch: u8) -> f32 {
        rect.bottom() - pitch as f32 * self.state.zoom_y + self.state.scroll_px.y
    }

    fn time_to_x(&self, rect: Rect, ppq: i64) -> f32 {
        let beats = ppq as f32 / self.state.ppq() as f32;
        rect.left() + beats * self.state.zoom_x - self.state.scroll_px.x
    }

    fn snap_label(&self) -> String {
        match self.state.snap {
            None => "Off".to_owned(),
            Some(SnapUnit::Bar) => "Bar".to_owned(),
            Some(SnapUnit::Beat) => "Beat".to_owned(),
            Some(SnapUnit::Grid(div)) => format!("1/{}", div),
        }
    }
}

fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

fn apply_velocity_tint(color: Color32, velocity: u8) -> Color32 {
    let factor = 0.55 + (velocity as f32 / 127.0) * 0.45;
    let r = (color.r() as f32 * factor).clamp(0.0, 255.0) as u8;
    let g = (color.g() as f32 * factor).clamp(0.0, 255.0) as u8;
    let b = (color.b() as f32 * factor).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, color.a())
}

#[cfg(feature = "demo-app")]
pub mod demo {
    use rand::Rng;

    use super::*;

    /// Runs a standalone eframe demo window showcasing the piano roll.
    pub fn run_demo() -> eframe::Result<()> {
        let mut clip = Clip::new(960);
        let mut rng = rand::thread_rng();
        for i in 0..256u64 {
            let start = (i as i64 * 240) % (960 * 16);
            let dur = 120 + (rng.gen::<u8>() as i64 % 240);
            let pitch = 36 + rng.gen::<u8>() % 48;
            clip.notes.push(Note {
                id: i,
                start_ppq: start,
                dur_ppq: dur,
                pitch,
                vel: 90,
                chan: 0,
                selected: false,
            });
        }
        clip.sort_notes();
        let state = EditorState::new(clip);
        eframe::run_native(
            "Harmoniq Piano Roll",
            eframe::NativeOptions::default(),
            Box::new(move |_cc| {
                Box::new(DemoApp {
                    roll: PianoRoll::new(state.clone()),
                })
            }),
        )
    }

    struct DemoApp {
        roll: PianoRoll,
    }

    impl eframe::App for DemoApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.roll.ui(ui);
                let edits = self.roll.take_edits();
                if !edits.is_empty() {
                    eprintln!("edits: {:?}", edits);
                }
            });
        }
    }
}
