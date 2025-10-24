use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense};
use harmoniq_engine::TransportState;
use harmoniq_ui::HarmoniqPalette;

use crate::ui::event_bus::EventBus;
use crate::ui::focus::InputFocus;
use crate::ui::inspector::InspectorCommand;
use crate::ui::workspace::WorkspacePane;
use crate::TransportClock;

#[derive(Debug, Clone)]
pub struct ClipSelection {
    pub track_index: usize,
    pub clip_index: usize,
    pub track_name: String,
    pub clip_name: String,
    pub start: f32,
    pub length: f32,
    pub color: Color32,
}

#[derive(Clone)]
struct Clip {
    id: u64,
    name: String,
    start: f32,
    length: f32,
    color: Color32,
}

impl Clip {
    fn end(&self) -> f32 {
        self.start + self.length
    }
}

#[derive(Clone)]
struct PlaylistTrack {
    name: String,
    color: Color32,
    clips: Vec<Clip>,
}

#[derive(Debug, Clone, Copy)]
enum SnapSetting {
    Bar,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

impl SnapSetting {
    fn label(self) -> &'static str {
        match self {
            SnapSetting::Bar => "1 bar",
            SnapSetting::Half => "1/2",
            SnapSetting::Quarter => "1/4",
            SnapSetting::Eighth => "1/8",
            SnapSetting::Sixteenth => "1/16",
        }
    }

    fn interval(self) -> f32 {
        match self {
            SnapSetting::Bar => 4.0,
            SnapSetting::Half => 2.0,
            SnapSetting::Quarter => 1.0,
            SnapSetting::Eighth => 0.5,
            SnapSetting::Sixteenth => 0.25,
        }
    }
}

#[derive(Debug, Clone)]
struct ClipDragState {
    clip_id: u64,
    from_track: usize,
    pointer_offset: f32,
    original_start: f32,
    target_track: usize,
}

#[derive(Debug, Clone)]
enum ClipOperation {
    Move {
        clip_id: u64,
        from_track: usize,
        to_track: usize,
        start: f32,
    },
    Duplicate {
        track_index: usize,
        clip_index: usize,
    },
    Delete {
        track_index: usize,
        clip_index: usize,
    },
}

pub struct PlaylistPane {
    tracks: Vec<PlaylistTrack>,
    playhead: f32,
    length_beats: f32,
    snap_enabled: bool,
    snap_setting: SnapSetting,
    selected_clip: Option<(usize, usize)>,
    drag_state: Option<ClipDragState>,
    pending_ops: Vec<ClipOperation>,
    next_clip_id: u64,
}

impl Default for PlaylistPane {
    fn default() -> Self {
        let mut pane = Self {
            tracks: Vec::new(),
            playhead: 0.0,
            length_beats: 64.0,
            snap_enabled: true,
            snap_setting: SnapSetting::Quarter,
            selected_clip: None,
            drag_state: None,
            pending_ops: Vec::new(),
            next_clip_id: 1,
        };
        pane.seed_demo_tracks();
        pane
    }
}

impl PlaylistPane {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        event_bus: &EventBus,
        focus: &mut InputFocus,
        transport_state: TransportState,
        clock: TransportClock,
    ) {
        let ctx = ui.ctx().clone();
        let mut root_rect = ui.min_rect();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Arrange").color(palette.text_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            clock.format(),
                            display_state(transport_state)
                        ))
                        .color(palette.text_muted),
                    );
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.snap_enabled, "Snap");
                egui::ComboBox::from_id_source("playlist_snap")
                    .selected_text(self.snap_setting.label())
                    .show_ui(ui, |ui| {
                        for setting in [
                            SnapSetting::Bar,
                            SnapSetting::Half,
                            SnapSetting::Quarter,
                            SnapSetting::Eighth,
                            SnapSetting::Sixteenth,
                        ] {
                            ui.selectable_value(&mut self.snap_setting, setting, setting.label());
                        }
                    });
                if ui.button("Add track").clicked() {
                    self.add_track();
                }
                if ui.button("New clip").clicked() {
                    self.spawn_clip_on_selected_track();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Zoom to fit").clicked() {
                        self.zoom_to_content();
                    }
                });
            });
            ui.add_space(8.0);

            let track_height = 68.0;
            let header_width = 180.0;
            let ruler_height = 28.0;
            let beat_width = 80.0;
            let total_height = track_height * self.tracks.len() as f32;
            let total_width = self.length_beats.max(8.0) * beat_width;

            let scroll_response = egui::ScrollArea::both()
                .id_source("playlist_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let size = egui::vec2(
                        total_width + header_width + 120.0,
                        total_height + ruler_height + 80.0,
                    );
                    let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
                    let rect = response.rect.shrink2(egui::vec2(60.0, 40.0));

                    let header_rect = Rect::from_min_max(
                        rect.min,
                        Pos2::new(rect.left() + header_width, rect.bottom()),
                    );
                    let ruler_rect = Rect::from_min_max(
                        Pos2::new(header_rect.right(), rect.top()),
                        Pos2::new(rect.right(), rect.top() + ruler_height),
                    );
                    let timeline_rect = Rect::from_min_max(
                        Pos2::new(header_rect.right(), ruler_rect.bottom()),
                        rect.max,
                    );

                    painter.rect_filled(header_rect, 12.0, palette.panel_alt);
                    painter.rect_filled(ruler_rect, 12.0, palette.timeline_header);
                    painter.rect_filled(timeline_rect, 12.0, palette.panel);

                    self.draw_tracks(
                        ui,
                        &painter,
                        palette,
                        header_rect,
                        timeline_rect,
                        beat_width,
                        track_height,
                    );
                    self.draw_ruler(&painter, palette, ruler_rect, beat_width);
                    self.draw_clips(
                        ui,
                        &painter,
                        palette,
                        timeline_rect,
                        beat_width,
                        track_height,
                        event_bus,
                    );
                    self.draw_playhead(&painter, palette, timeline_rect, beat_width);
                    response.context_menu(|ui| {
                        if ui.button("Insert empty clip").clicked() {
                            if let Some(pos) = response.interact_pointer_pos() {
                                self.insert_empty_clip_at(
                                    pos,
                                    timeline_rect,
                                    beat_width,
                                    track_height,
                                );
                            }
                            ui.close_menu();
                        }
                        if ui.button("Clear selection").clicked() {
                            self.selected_clip = None;
                            ui.close_menu();
                        }
                    });
                });

            root_rect = root_rect.union(scroll_response.inner_rect);
        });

        self.flush_operations();
        focus.track_pane_interaction(&ctx, root_rect, WorkspacePane::Arrange);
    }

    pub fn current_selection(&self) -> Option<ClipSelection> {
        self.selected_clip.map(|(track_index, clip_index)| {
            let track = &self.tracks[track_index];
            let clip = &track.clips[clip_index];
            ClipSelection {
                track_index,
                clip_index,
                track_name: track.name.clone(),
                clip_name: clip.name.clone(),
                start: clip.start,
                length: clip.length,
                color: clip.color,
            }
        })
    }

    pub fn apply_inspector_command(&mut self, command: InspectorCommand) {
        match command {
            InspectorCommand::RenameClip {
                track_index,
                clip_index,
                name,
            } => {
                if let Some(clip) = self
                    .tracks
                    .get_mut(track_index)
                    .and_then(|track| track.clips.get_mut(clip_index))
                {
                    clip.name = name;
                }
            }
            InspectorCommand::UpdateClipRange {
                track_index,
                clip_index,
                start,
                length,
            } => {
                if let Some(clip) = self
                    .tracks
                    .get_mut(track_index)
                    .and_then(|track| track.clips.get_mut(clip_index))
                {
                    clip.start = self.snap(start.max(0.0));
                    clip.length = length.max(0.125);
                }
            }
            InspectorCommand::DeleteClip {
                track_index,
                clip_index,
            } => {
                if let Some(track) = self.tracks.get_mut(track_index) {
                    if clip_index < track.clips.len() {
                        track.clips.remove(clip_index);
                    }
                }
                if self.selected_clip == Some((track_index, clip_index)) {
                    self.selected_clip = None;
                }
            }
            InspectorCommand::DuplicateClip {
                track_index,
                clip_index,
            } => {
                if let Some(track) = self.tracks.get_mut(track_index) {
                    if let Some(source) = track.clips.get(clip_index).cloned() {
                        let mut clone = source.clone();
                        clone.start = source.end();
                        clone.id = self.allocate_id();
                        track.clips.insert(clip_index + 1, clone);
                    }
                }
            }
        }
    }

    pub fn playhead_position(&self) -> f32 {
        self.playhead
    }

    pub fn set_playhead(&mut self, beats: f32, playing: bool) {
        if self.length_beats <= 0.0 {
            self.playhead = 0.0;
            return;
        }
        let cycle = self.length_beats.max(1.0);
        let position = if playing {
            beats.rem_euclid(cycle)
        } else {
            beats
        };
        self.playhead = position.clamp(0.0, cycle);
    }

    fn draw_tracks(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        header_rect: Rect,
        timeline_rect: Rect,
        beat_width: f32,
        track_height: f32,
    ) {
        for (index, track) in self.tracks.iter().enumerate() {
            let top = timeline_rect.top() + index as f32 * track_height;
            let row_rect = Rect::from_min_max(
                Pos2::new(timeline_rect.left(), top),
                Pos2::new(timeline_rect.right(), top + track_height),
            );
            painter.rect_filled(row_rect, 6.0, palette.panel_alt.gamma_multiply(1.04));
            painter.rect_stroke(
                row_rect,
                6.0,
                egui::Stroke::new(1.0, palette.timeline_grid_secondary),
            );

            let header_y = header_rect.top() + index as f32 * track_height;
            let header_row = Rect::from_min_max(
                Pos2::new(header_rect.left(), header_y),
                Pos2::new(header_rect.right(), header_y + track_height),
            );
            painter.rect_filled(header_row, 6.0, palette.panel_alt.gamma_multiply(0.95));
            painter.rect_stroke(
                header_row,
                6.0,
                egui::Stroke::new(1.0, palette.timeline_grid_secondary),
            );
            painter.text(
                Pos2::new(header_row.left() + 18.0, header_row.center().y),
                egui::Align2::LEFT_CENTER,
                &track.name,
                egui::FontId::proportional(16.0),
                track.color,
            );
        }

        if let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) {
            if timeline_rect.contains(pointer) {
                let beat = (pointer.x - timeline_rect.left()) / beat_width;
                let snapped = if self.snap_enabled {
                    self.snap(beat)
                } else {
                    beat
                };
                let x = timeline_rect.left() + snapped * beat_width;
                painter.line_segment(
                    [
                        Pos2::new(x, timeline_rect.top()),
                        Pos2::new(x, timeline_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, palette.timeline_grid_primary),
                );
            }
        }
    }

    fn draw_ruler(
        &self,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        rect: Rect,
        beat_width: f32,
    ) {
        let bars = (self.length_beats / 4.0).ceil() as i32;
        for bar in 0..=bars {
            let beat_index = bar as f32 * 4.0;
            let x = rect.left() + beat_index * beat_width;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                egui::Stroke::new(1.5, palette.timeline_grid_primary),
            );
            painter.text(
                Pos2::new(x + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                format!("Bar {}", bar + 1),
                egui::FontId::proportional(12.0),
                palette.ruler_text,
            );
            for beat in 1..4 {
                if bar == bars && beat_index + beat as f32 > self.length_beats {
                    break;
                }
                let beat_x = x + beat as f32 * beat_width;
                painter.line_segment(
                    [
                        Pos2::new(beat_x, rect.bottom() - 6.0),
                        Pos2::new(beat_x, rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, palette.timeline_grid_secondary),
                );
            }
        }
    }

    fn draw_clips(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        timeline_rect: Rect,
        beat_width: f32,
        track_height: f32,
        event_bus: &EventBus,
    ) {
        for track_index in 0..self.tracks.len() {
            let clips_len = self.tracks[track_index].clips.len();
            for clip_index in 0..clips_len {
                let clip = self.tracks[track_index].clips[clip_index].clone();
                let clip_id = clip.id;
                let x = timeline_rect.left() + clip.start * beat_width;
                let width = (clip.length * beat_width).max(12.0);
                let top = timeline_rect.top() + track_index as f32 * track_height;
                let clip_rect = Rect::from_min_size(
                    Pos2::new(x, top + 6.0),
                    egui::vec2(width, track_height - 12.0),
                );
                let id = ui.make_persistent_id((clip_id, track_index, clip_index));
                let clip_response = ui.interact(clip_rect, id, Sense::click_and_drag());

                let selected = self.selected_clip == Some((track_index, clip_index));
                let fill = if selected {
                    clip.color.gamma_multiply(1.15)
                } else {
                    clip.color.gamma_multiply(0.85)
                };
                painter.rect_filled(clip_rect, 10.0, fill);
                painter.rect_stroke(
                    clip_rect,
                    10.0,
                    egui::Stroke::new(
                        1.2,
                        if selected {
                            palette.clip_border_active
                        } else {
                            palette.clip_border_default
                        },
                    ),
                );
                painter.text(
                    clip_rect.left_top() + egui::vec2(10.0, 18.0),
                    egui::Align2::LEFT_TOP,
                    &clip.name,
                    egui::FontId::proportional(14.0),
                    palette.clip_text_primary,
                );

                if clip_response.clicked() {
                    self.selected_clip = Some((track_index, clip_index));
                    event_bus.publish(crate::ui::event_bus::AppEvent::RequestRepaint);
                }

                self.handle_drag(
                    ui,
                    clip_response,
                    clip_rect,
                    track_index,
                    clip_index,
                    clip_id,
                    timeline_rect,
                    beat_width,
                    track_height,
                );

                clip_response.context_menu(|ui| {
                    if ui.button("Duplicate").clicked() {
                        self.pending_ops.push(ClipOperation::Duplicate {
                            track_index,
                            clip_index,
                        });
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        self.pending_ops.push(ClipOperation::Delete {
                            track_index,
                            clip_index,
                        });
                        ui.close_menu();
                    }
                });
            }
        }
    }

    fn draw_playhead(
        &self,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        timeline_rect: Rect,
        beat_width: f32,
    ) {
        let playhead_x = timeline_rect.left() + self.playhead * beat_width;
        painter.line_segment(
            [
                Pos2::new(playhead_x, timeline_rect.top()),
                Pos2::new(playhead_x, timeline_rect.bottom()),
            ],
            egui::Stroke::new(2.0, palette.accent),
        );
    }

    fn handle_drag(
        &mut self,
        ui: &egui::Ui,
        response: egui::Response,
        clip_rect: Rect,
        track_index: usize,
        clip_index: usize,
        clip_id: u64,
        timeline_rect: Rect,
        beat_width: f32,
        track_height: f32,
    ) {
        if response.drag_started() {
            if let Some(pointer) = response.interact_pointer_pos() {
                self.drag_state = Some(ClipDragState {
                    clip_id,
                    from_track: track_index,
                    pointer_offset: pointer.x - clip_rect.left(),
                    original_start: self.tracks[track_index].clips[clip_index].start,
                    target_track: track_index,
                });
            }
        }

        if response.dragged() {
            if let (Some(pointer), Some(state)) =
                (response.interact_pointer_pos(), self.drag_state.as_mut())
            {
                if state.clip_id != clip_id {
                    return;
                }
                let mut beat =
                    (pointer.x - state.pointer_offset - timeline_rect.left()) / beat_width;
                beat = beat.max(0.0);
                let snapped = self.snap(beat);
                if let Some(clip) = self.tracks[state.from_track]
                    .clips
                    .iter_mut()
                    .find(|clip| clip.id == clip_id)
                {
                    clip.start = snapped;
                }

                let track = self.track_at(pointer.y, timeline_rect, track_height);
                state.target_track = track;
            }
        }

        if response.drag_released() {
            if let Some(state) = self.drag_state.take() {
                let new_start = self
                    .tracks
                    .get(state.from_track)
                    .and_then(|track| track.clips.iter().find(|clip| clip.id == state.clip_id))
                    .map(|clip| clip.start)
                    .unwrap_or(state.original_start);

                if state.target_track != state.from_track {
                    self.pending_ops.push(ClipOperation::Move {
                        clip_id: state.clip_id,
                        from_track: state.from_track,
                        to_track: state.target_track,
                        start: new_start,
                    });
                }
            }
        }
    }

    fn track_at(&self, pointer_y: f32, timeline_rect: Rect, track_height: f32) -> usize {
        (((pointer_y - timeline_rect.top()) / track_height).floor() as isize)
            .clamp(0, self.tracks.len() as isize - 1) as usize
    }

    fn snap(&self, beat: f32) -> f32 {
        if !self.snap_enabled {
            beat
        } else {
            let step = self.snap_setting.interval();
            (beat / step).round() * step
        }
    }

    fn flush_operations(&mut self) {
        let ops = std::mem::take(&mut self.pending_ops);
        for op in ops {
            match op {
                ClipOperation::Move {
                    clip_id,
                    from_track,
                    to_track,
                    start,
                } => {
                    if from_track == to_track {
                        continue;
                    }
                    if let Some(track) = self.tracks.get_mut(from_track) {
                        if let Some(index) = track.clips.iter().position(|clip| clip.id == clip_id)
                        {
                            let clip = track.clips.remove(index);
                            if let Some(target_track) = self.tracks.get_mut(to_track) {
                                let mut moved = clip;
                                moved.start = self.snap(start);
                                target_track.clips.push(moved);
                                target_track
                                    .clips
                                    .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
                                if let Some(idx) = target_track
                                    .clips
                                    .iter()
                                    .position(|clip| clip.id == clip_id)
                                {
                                    self.selected_clip = Some((to_track, idx));
                                }
                            }
                        }
                    }
                }
                ClipOperation::Duplicate {
                    track_index,
                    clip_index,
                } => {
                    if let Some(track) = self.tracks.get_mut(track_index) {
                        if let Some(source) = track.clips.get(clip_index).cloned() {
                            let mut clone = source.clone();
                            clone.id = self.allocate_id();
                            clone.start = self.snap(source.end());
                            track.clips.insert(clip_index + 1, clone);
                            self.selected_clip = Some((track_index, clip_index + 1));
                        }
                    }
                }
                ClipOperation::Delete {
                    track_index,
                    clip_index,
                } => {
                    if let Some(track) = self.tracks.get_mut(track_index) {
                        if clip_index < track.clips.len() {
                            track.clips.remove(clip_index);
                        }
                    }
                    if self.selected_clip == Some((track_index, clip_index)) {
                        self.selected_clip = None;
                    }
                }
            }
        }
    }

    fn insert_empty_clip_at(
        &mut self,
        pos: Pos2,
        timeline_rect: Rect,
        beat_width: f32,
        track_height: f32,
    ) {
        if !timeline_rect.contains(pos) {
            return;
        }
        let track_index = self.track_at(pos.y, timeline_rect, track_height);
        let beat = (pos.x - timeline_rect.left()) / beat_width;
        let start = self.snap(beat).max(0.0);
        let color = self
            .tracks
            .get(track_index)
            .map(|t| t.color)
            .unwrap_or(Color32::from_rgb(120, 160, 220));
        let clip = Clip {
            id: self.allocate_id(),
            name: "New Clip".into(),
            start,
            length: 4.0,
            color,
        };
        if let Some(track) = self.tracks.get_mut(track_index) {
            track.clips.push(clip);
            track
                .clips
                .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
            self.selected_clip = Some((track_index, track.clips.len() - 1));
        }
    }

    fn add_track(&mut self) {
        let palette = [
            Color32::from_rgb(240, 170, 100),
            Color32::from_rgb(150, 140, 220),
            Color32::from_rgb(130, 200, 240),
            Color32::from_rgb(120, 200, 160),
        ];
        let index = self.tracks.len();
        self.tracks.push(PlaylistTrack {
            name: format!("Track {}", index + 1),
            color: palette[index % palette.len()],
            clips: Vec::new(),
        });
    }

    fn spawn_clip_on_selected_track(&mut self) {
        let track_index = self.selected_clip.map(|(track, _)| track).unwrap_or(0);
        if self.tracks.is_empty() {
            self.add_track();
        }
        let track = &mut self.tracks[track_index];
        let start = track
            .clips
            .last()
            .map(|clip| self.snap(clip.end()))
            .unwrap_or(0.0);
        track.clips.push(Clip {
            id: self.allocate_id(),
            name: format!("Clip {}", track.clips.len() + 1),
            start,
            length: 4.0,
            color: track.color,
        });
        self.selected_clip = Some((track_index, track.clips.len() - 1));
    }

    fn zoom_to_content(&mut self) {
        let max_end = self
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(|clip| clip.end())
            .fold(16.0, f32::max);
        self.length_beats = max_end.max(8.0);
    }

    fn seed_demo_tracks(&mut self) {
        let mut tracks = vec![
            PlaylistTrack {
                name: "Drums".into(),
                color: Color32::from_rgb(240, 170, 100),
                clips: Vec::new(),
            },
            PlaylistTrack {
                name: "Bass".into(),
                color: Color32::from_rgb(150, 140, 220),
                clips: Vec::new(),
            },
            PlaylistTrack {
                name: "Lead".into(),
                color: Color32::from_rgb(130, 200, 240),
                clips: Vec::new(),
            },
        ];
        let mut id = self.next_clip_id;
        for (index, track) in tracks.iter_mut().enumerate() {
            for n in 0..3 {
                track.clips.push(Clip {
                    id,
                    name: format!("{} Clip {}", track.name, n + 1),
                    start: (n * 4) as f32 + index as f32,
                    length: 4.0,
                    color: track.color,
                });
                id += 1;
            }
        }
        self.next_clip_id = id;
        self.tracks = tracks;
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_clip_id;
        self.next_clip_id += 1;
        id
    }
}

fn display_state(state: TransportState) -> &'static str {
    match state {
        TransportState::Playing => "Playing",
        TransportState::Recording => "Recording",
        TransportState::Stopped => "Stopped",
    }
}
