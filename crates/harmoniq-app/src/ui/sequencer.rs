use std::collections::HashMap;

use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, RichText, Shape, Stroke,
};
use harmoniq_engine::TransportState;
use harmoniq_ui::HarmoniqPalette;

use crate::ui::event_bus::{AppEvent, EventBus};
use crate::ui::piano_roll::{PianoRollNote, PianoRollPattern};
use crate::{TimeSignature, TransportClock};

use super::focus::InputFocus;
use super::workspace::WorkspacePane;

const TRACK_LIST_WIDTH: f32 = 220.0;
const TRACK_ROW_HEIGHT: f32 = 82.0;
const HEADER_HEIGHT: f32 = 34.0;
const BEAT_WIDTH: f32 = 68.0;
const CLIP_PADDING: f32 = 10.0;
const LOOP_ALPHA: f32 = 0.16;

#[derive(Clone)]
struct AutomationPoint {
    position: f32,
    value: f32,
}

#[derive(Clone)]
struct AutomationLane {
    parameter: String,
    points: Vec<AutomationPoint>,
    visible: bool,
}

#[derive(Clone)]
struct SequencerClip {
    id: u64,
    start: f32,
    length: f32,
    name: String,
    color: Color32,
    pattern_id: u32,
}

impl SequencerClip {
    fn end(&self) -> f32 {
        self.start + self.length
    }
}

#[derive(Clone)]
struct SequencerTrack {
    name: String,
    color: Color32,
    clips: Vec<SequencerClip>,
    automation: Vec<AutomationLane>,
    muted: bool,
    solo: bool,
}

impl SequencerTrack {
    fn header_color(&self, palette: &HarmoniqPalette, selected: bool) -> Color32 {
        let mut base = palette.panel;
        if selected {
            base = base.gamma_multiply(1.12);
        }
        base
    }
}

#[derive(Clone)]
struct StockArrangement {
    name: String,
    tempo: u32,
    bars: u32,
    description: String,
    tags: Vec<String>,
}

impl StockArrangement {
    fn summary(&self) -> String {
        format!("{} bars • {} BPM", self.bars, self.tempo)
    }
}

#[derive(Clone, Default)]
struct PatternLibrary {
    patterns: HashMap<u32, PianoRollPattern>,
    next_id: u32,
}

impl PatternLibrary {
    fn register_pattern(&mut self, mut pattern: PianoRollPattern) -> u32 {
        if pattern.id == 0 {
            pattern.id = self.allocate_id();
        } else {
            self.next_id = self.next_id.max(pattern.id + 1);
        }
        let id = pattern.id;
        self.patterns.insert(id, pattern);
        id
    }

    fn allocate_id(&mut self) -> u32 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    fn pattern(&self, id: u32) -> Option<PianoRollPattern> {
        self.patterns.get(&id).cloned()
    }

    fn ensure_blank(&mut self, name: impl Into<String>) -> u32 {
        let id = self.allocate_id();
        let pattern = PianoRollPattern {
            id,
            name: name.into(),
            notes: vec![PianoRollNote {
                start: 0.0,
                length: 1.0,
                pitch: 60,
                velocity: 0.8,
            }],
        };
        self.patterns.insert(id, pattern);
        id
    }
}

#[derive(Clone, Copy)]
struct ClipDragState {
    track_index: usize,
    clip_id: u64,
    grab_offset_beats: f32,
}

#[derive(Clone, Copy)]
struct AutomationDragState {
    track_index: usize,
    lane_index: usize,
    point_index: usize,
}

#[derive(Clone, Copy)]
struct ContextMenuRequest {
    track_index: usize,
    beat: f32,
    pointer: Pos2,
}

pub struct SequencerPane {
    tracks: Vec<SequencerTrack>,
    selected_track: Option<usize>,
    selected_clip: Option<(usize, usize)>,
    snap_enabled: bool,
    follow_playhead: bool,
    show_sub_beats: bool,
    automation_overlay: bool,
    loop_region: Option<(f32, f32)>,
    zoom: f32,
    total_bars: u32,
    stock_arrangements: Vec<StockArrangement>,
    selected_stock_arrangement: Option<usize>,
    pattern_library: PatternLibrary,
    next_clip_id: u64,
    pending_context_menu: Option<ContextMenuRequest>,
    dragging_clip: Option<ClipDragState>,
    dragging_automation: Option<AutomationDragState>,
}

impl Default for SequencerPane {
    fn default() -> Self {
        let mut pattern_library = PatternLibrary::default();
        let mut next_clip_id = 1;
        let tracks = demo_tracks(&mut pattern_library, &mut next_clip_id);
        Self {
            tracks,
            selected_track: Some(0),
            selected_clip: Some((0, 0)),
            snap_enabled: true,
            follow_playhead: true,
            show_sub_beats: true,
            automation_overlay: true,
            loop_region: Some((4.0, 8.0)),
            zoom: 1.0,
            total_bars: 8,
            stock_arrangements: stock_arrangements(),
            selected_stock_arrangement: None,
            pattern_library,
            next_clip_id,
            pending_context_menu: None,
            dragging_clip: None,
            dragging_automation: None,
        }
    }
}

impl SequencerPane {
    pub fn pattern(&self, pattern_id: u32) -> Option<PianoRollPattern> {
        self.pattern_library.pattern(pattern_id)
    }

    fn allocate_clip_id(&mut self) -> u64 {
        let id = self.next_clip_id;
        self.next_clip_id += 1;
        id
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        time_signature: TimeSignature,
        transport_clock: TransportClock,
        transport_state: TransportState,
        focus: Option<&mut InputFocus>,
        event_bus: &EventBus,
    ) {
        let ctx = ui.ctx().clone();
        let mut root_rect = ui.min_rect();

        ui.vertical(|ui| {
            ui.heading(RichText::new("Sequencer").color(palette.text_primary));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.snap_enabled, "Snap to grid");
                ui.checkbox(&mut self.follow_playhead, "Follow playhead");
                ui.checkbox(&mut self.show_sub_beats, "Sub-beat grid");
                ui.checkbox(&mut self.automation_overlay, "Automation overlay");
                ui.label(RichText::new("Zoom").color(palette.text_muted));
                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.6..=2.4)
                        .text("")
                        .logarithmic(true),
                );
                let mut loop_enabled = self.loop_region.is_some();
                if ui.checkbox(&mut loop_enabled, "Loop region").changed() {
                    if loop_enabled && self.loop_region.is_none() {
                        self.loop_region = Some((4.0, 8.0));
                    } else if !loop_enabled {
                        self.loop_region = None;
                    }
                }
                if let Some((mut start, mut end)) = self.loop_region {
                    let beats_per_bar = time_signature.numerator.max(1) as f32;
                    let total_beats = self.total_bars as f32 * beats_per_bar;
                    let mut changed = false;
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut start)
                                .clamp_range(0.0..=total_beats)
                                .speed(0.25)
                                .suffix(" beat"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut end)
                                .clamp_range(0.0..=total_beats)
                                .speed(0.25)
                                .suffix(" beat"),
                        )
                        .changed();
                    if changed {
                        if end < start {
                            std::mem::swap(&mut start, &mut end);
                        }
                        let min_length = 0.5;
                        let max_start = (total_beats - min_length).max(0.0);
                        start = start.clamp(0.0, max_start);
                        end = end.max(start + min_length).min(total_beats);
                        self.loop_region = Some((start, end));
                    }
                }
            });
            ui.add_space(8.0);

            self.draw_stock_arrangements(ui, palette, event_bus);
            ui.add_space(8.0);

            let scroll = egui::ScrollArea::both()
                .id_source("sequencer_scroll")
                .show(ui, |ui| {
                    self.draw_arrangement(
                        ui,
                        palette,
                        time_signature,
                        transport_clock,
                        transport_state,
                        event_bus,
                    )
                });
            root_rect = root_rect.union(scroll.inner_rect);
        });

        if let Some(focus) = focus {
            focus.track_pane_interaction(&ctx, root_rect, WorkspacePane::Sequencer);
        }
    }

    fn draw_stock_arrangements(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        event_bus: &EventBus,
    ) {
        egui::CollapsingHeader::new(
            RichText::new("Stock Sequencer Sounds").color(palette.text_primary),
        )
        .id_source("sequencer_stock_sounds")
        .default_open(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for (index, arrangement) in self.stock_arrangements.iter().enumerate() {
                let selected = self.selected_stock_arrangement == Some(index);
                let fill = if selected {
                    palette.panel_alt.gamma_multiply(1.08)
                } else {
                    palette.panel_alt
                };
                egui::Frame::none()
                    .fill(fill)
                    .rounding(egui::Rounding::same(10.0))
                    .stroke(egui::Stroke::new(1.0, palette.toolbar_outline))
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let response = ui.selectable_label(
                                selected,
                                RichText::new(&arrangement.name)
                                    .color(palette.text_primary)
                                    .strong()
                                    .size(16.0),
                            );
                            if ui
                                .add_sized(
                                    [82.0, 26.0],
                                    egui::Button::new(RichText::new("Preview").size(13.0)),
                                )
                                .clicked()
                            {
                                event_bus
                                    .publish(AppEvent::PreviewStockSound(arrangement.name.clone()));
                            }
                            if response.clicked() {
                                if selected {
                                    self.selected_stock_arrangement = None;
                                } else {
                                    self.selected_stock_arrangement = Some(index);
                                }
                            }
                        });
                        ui.label(RichText::new(arrangement.summary()).color(palette.text_muted));
                        ui.add_space(4.0);
                        ui.label(RichText::new(&arrangement.description).color(palette.text_muted));
                        if !arrangement.tags.is_empty() {
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for tag in &arrangement.tags {
                                    let tag_text =
                                        RichText::new(tag).color(palette.text_primary).size(12.0);
                                    ui.label(tag_text);
                                }
                            });
                        }
                    });
            }
        });
    }

    fn draw_arrangement(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        time_signature: TimeSignature,
        transport_clock: TransportClock,
        transport_state: TransportState,
        event_bus: &EventBus,
    ) {
        let beats_per_bar = time_signature.numerator.max(1) as f32;
        let total_beats = self.total_bars as f32 * beats_per_bar;
        let beat_width = BEAT_WIDTH * self.zoom;
        let total_height = HEADER_HEIGHT + TRACK_ROW_HEIGHT * self.tracks.len() as f32 + 64.0;
        let total_width = TRACK_LIST_WIDTH + beat_width * total_beats + 200.0;

        let (response, painter) = ui.allocate_painter(
            egui::vec2(total_width, total_height),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;

        let header_rect = Rect::from_min_max(
            rect.min,
            Pos2::new(rect.right(), rect.top() + HEADER_HEIGHT),
        );
        let track_list_rect = Rect::from_min_max(
            Pos2::new(rect.left(), header_rect.bottom()),
            Pos2::new(rect.left() + TRACK_LIST_WIDTH, rect.bottom()),
        );
        let arrangement_rect = Rect::from_min_max(
            Pos2::new(track_list_rect.right(), header_rect.bottom()),
            rect.max,
        );

        painter.rect_filled(rect, 10.0, palette.panel_alt);
        painter.rect_filled(header_rect, 10.0, palette.panel);
        painter.rect_filled(track_list_rect, 10.0, palette.panel);
        painter.rect_stroke(rect, 10.0, Stroke::new(1.0, palette.toolbar_outline));

        for bar in 0..=self.total_bars {
            let x = arrangement_rect.left() + bar as f32 * beats_per_bar * beat_width;
            painter.line_segment(
                [
                    Pos2::new(x, header_rect.top()),
                    Pos2::new(x, arrangement_rect.bottom()),
                ],
                Stroke::new(1.6, palette.timeline_grid_primary),
            );
            if bar < self.total_bars {
                painter.text(
                    Pos2::new(x + 6.0, header_rect.top() + 8.0),
                    Align2::LEFT_TOP,
                    format!("Bar {}", bar + 1),
                    FontId::proportional(14.0),
                    palette.text_muted,
                );
            }
            if self.show_sub_beats {
                for beat in 1..time_signature.numerator.max(1) {
                    let beat_x = arrangement_rect.left()
                        + (bar as f32 * beats_per_bar + beat as f32) * beat_width;
                    painter.line_segment(
                        [
                            Pos2::new(beat_x, header_rect.bottom()),
                            Pos2::new(beat_x, arrangement_rect.bottom()),
                        ],
                        Stroke::new(0.7, palette.timeline_grid_secondary),
                    );
                }
            }
        }

        for track_index in 0..=self.tracks.len() {
            let y = track_list_rect.top() + track_index as f32 * TRACK_ROW_HEIGHT;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, palette.timeline_grid_secondary),
            );
        }

        if let Some((start, end)) = self.loop_region {
            let start_x = arrangement_rect.left() + start * beat_width;
            let end_x = arrangement_rect.left() + end * beat_width;
            let loop_rect = Rect::from_min_max(
                Pos2::new(start_x, header_rect.bottom()),
                Pos2::new(end_x, arrangement_rect.bottom()),
            );
            let color = palette.accent.linear_multiply(LOOP_ALPHA);
            painter.rect_filled(loop_rect, 4.0, color);
            painter.rect_stroke(loop_rect, 4.0, Stroke::new(1.0, palette.accent));
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(start_x, header_rect.bottom()),
                    Pos2::new(start_x + 10.0, header_rect.bottom() - 12.0),
                    Pos2::new(start_x + 20.0, header_rect.bottom()),
                ],
                palette.accent,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(end_x, header_rect.bottom()),
                    Pos2::new(end_x - 10.0, header_rect.bottom() - 12.0),
                    Pos2::new(end_x - 20.0, header_rect.bottom()),
                ],
                palette.accent,
                Stroke::NONE,
            ));
        }

        let playhead_ticks = transport_clock.total_ticks(time_signature);
        let playhead_beats = playhead_ticks as f32 / 960.0;
        let playhead_x = arrangement_rect.left() + playhead_beats * beat_width;
        if arrangement_rect.left() <= playhead_x && playhead_x <= arrangement_rect.right() {
            if self.follow_playhead
                && matches!(
                    transport_state,
                    TransportState::Playing | TransportState::Recording
                )
            {
                let half_band = (beat_width * 0.25).max(12.0);
                let follow_rect = Rect::from_min_max(
                    Pos2::new(
                        (playhead_x - half_band).max(arrangement_rect.left()),
                        header_rect.bottom(),
                    ),
                    Pos2::new(
                        (playhead_x + half_band).min(arrangement_rect.right()),
                        arrangement_rect.bottom(),
                    ),
                );
                painter.rect_filled(follow_rect, 4.0, palette.accent.linear_multiply(0.08));
            }
            let color = if matches!(
                transport_state,
                TransportState::Playing | TransportState::Recording
            ) {
                palette.accent
            } else {
                palette.timeline_grid_primary
            };
            painter.line_segment(
                [
                    Pos2::new(playhead_x, header_rect.top()),
                    Pos2::new(playhead_x, arrangement_rect.bottom()),
                ],
                Stroke::new(2.0, color),
            );
        }

        self.draw_tracks(
            ui,
            &painter,
            palette,
            track_list_rect,
            arrangement_rect,
            beat_width,
            transport_state,
        );

        self.handle_pointer_interaction(
            ui,
            &response,
            track_list_rect,
            arrangement_rect,
            beat_width,
            beats_per_bar,
            total_beats,
            event_bus,
        );
        self.show_context_menu(ui, beats_per_bar, total_beats);
    }

    fn draw_tracks(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        track_list_rect: Rect,
        arrangement_rect: Rect,
        beat_width: f32,
        transport_state: TransportState,
    ) {
        for (track_index, track) in self.tracks.iter().enumerate() {
            let top = track_list_rect.top() + track_index as f32 * TRACK_ROW_HEIGHT;
            let header_rect = Rect::from_min_max(
                Pos2::new(track_list_rect.left(), top),
                Pos2::new(track_list_rect.right(), top + TRACK_ROW_HEIGHT),
            );
            let arrangement_row = Rect::from_min_max(
                Pos2::new(arrangement_rect.left(), top),
                Pos2::new(arrangement_rect.right(), top + TRACK_ROW_HEIGHT),
            );

            let selected = self.selected_track == Some(track_index);
            painter.rect_filled(header_rect, 8.0, track.header_color(palette, selected));
            painter.rect_stroke(
                header_rect,
                8.0,
                Stroke::new(
                    1.0,
                    if selected {
                        palette.accent
                    } else {
                        palette.timeline_grid_secondary
                    },
                ),
            );

            let color_rect = Rect::from_min_max(
                Pos2::new(header_rect.left() + 12.0, header_rect.top() + 16.0),
                Pos2::new(header_rect.left() + 24.0, header_rect.bottom() - 16.0),
            );
            painter.rect_filled(color_rect, 4.0, track.color);

            painter.text(
                Pos2::new(color_rect.right() + 12.0, header_rect.center().y),
                Align2::LEFT_CENTER,
                &track.name,
                FontId::proportional(16.0),
                palette.text_primary,
            );

            self.draw_track_buttons(painter, palette, &header_rect, track, track_index);
            self.draw_clips(
                ui,
                painter,
                palette,
                track,
                arrangement_row,
                beat_width,
                track_index,
            );
            if self.automation_overlay {
                self.draw_automation(painter, palette, track, arrangement_row, beat_width);
            }

            if matches!(transport_state, TransportState::Recording) && selected {
                let recording_hint = Rect::from_min_max(
                    Pos2::new(
                        arrangement_row.right() - 140.0,
                        arrangement_row.top() + 14.0,
                    ),
                    Pos2::new(arrangement_row.right() - 16.0, arrangement_row.top() + 34.0),
                );
                painter.rect_filled(recording_hint, 6.0, palette.accent.linear_multiply(0.2));
                painter.rect_stroke(recording_hint, 6.0, Stroke::new(1.0, palette.accent));
                painter.text(
                    recording_hint.center(),
                    Align2::CENTER_CENTER,
                    "Recording enabled",
                    FontId::proportional(13.0),
                    palette.accent,
                );
            }
        }
    }

    fn draw_track_buttons(
        &self,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        header_rect: &Rect,
        track: &SequencerTrack,
        track_index: usize,
    ) {
        let (solo_rect, mute_rect) = track_button_rects(header_rect);

        let solo_color = if track.solo {
            palette.accent
        } else {
            palette.panel_alt
        };
        let mute_color = if track.muted {
            palette.timeline_grid_secondary
        } else {
            palette.panel_alt
        };
        painter.rect_filled(solo_rect, 6.0, solo_color);
        painter.rect_stroke(
            solo_rect,
            6.0,
            Stroke::new(1.0, palette.timeline_grid_secondary),
        );
        painter.text(
            solo_rect.center(),
            Align2::CENTER_CENTER,
            "S",
            FontId::proportional(14.0),
            palette.text_primary,
        );

        painter.rect_filled(mute_rect, 6.0, mute_color);
        painter.rect_stroke(
            mute_rect,
            6.0,
            Stroke::new(1.0, palette.timeline_grid_secondary),
        );
        painter.text(
            mute_rect.center(),
            Align2::CENTER_CENTER,
            "M",
            FontId::proportional(14.0),
            palette.text_primary,
        );

        if self.selected_track == Some(track_index) {
            painter.rect_stroke(*header_rect, 8.0, Stroke::new(1.4, palette.accent));
        }
    }

    fn button_hit_rects(&self, track_index: usize, track_list_rect: &Rect) -> (Rect, Rect) {
        let top = track_list_rect.top() + track_index as f32 * TRACK_ROW_HEIGHT;
        let header_rect = Rect::from_min_max(
            Pos2::new(track_list_rect.left(), top),
            Pos2::new(track_list_rect.right(), top + TRACK_ROW_HEIGHT),
        );
        track_button_rects(&header_rect)
    }

    fn draw_clips(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        track: &SequencerTrack,
        row_rect: Rect,
        beat_width: f32,
        track_index: usize,
    ) {
        for (clip_index, clip) in track.clips.iter().enumerate() {
            let left = row_rect.left() + clip.start * beat_width + CLIP_PADDING;
            let right = row_rect.left() + clip.end() * beat_width - CLIP_PADDING;
            let rect = Rect::from_min_max(
                Pos2::new(left, row_rect.top() + 12.0),
                Pos2::new(right.max(left + 24.0), row_rect.bottom() - 12.0),
            );
            let selected = self.selected_clip == Some((track_index, clip_index));
            let fill = if selected {
                clip.color.gamma_multiply(1.15)
            } else {
                clip.color
            };
            painter.rect_filled(rect, 8.0, fill);
            painter.rect_stroke(
                rect,
                8.0,
                Stroke::new(
                    if selected { 2.0 } else { 1.2 },
                    if selected {
                        palette.accent
                    } else {
                        palette.timeline_grid_secondary
                    },
                ),
            );
            painter.text(
                Pos2::new(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                &clip.name,
                FontId::proportional(14.0),
                palette.text_primary,
            );

            let fade_length = 12.0;
            let fade_rect = Rect::from_min_max(
                Pos2::new(rect.left(), rect.top()),
                Pos2::new(rect.left() + fade_length, rect.bottom()),
            );
            painter.rect_filled(fade_rect, 8.0, fill.gamma_multiply(0.7));
            let fade_out_rect = Rect::from_min_max(
                Pos2::new(rect.right() - fade_length, rect.top()),
                Pos2::new(rect.right(), rect.bottom()),
            );
            painter.rect_filled(fade_out_rect, 8.0, fill.gamma_multiply(0.7));

            if ui.is_rect_visible(rect) {
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + fade_length, rect.top()),
                        Pos2::new(rect.left(), rect.bottom()),
                    ],
                    Stroke::new(1.0, palette.timeline_grid_secondary),
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.right() - fade_length, rect.top()),
                        Pos2::new(rect.right(), rect.bottom()),
                    ],
                    Stroke::new(1.0, palette.timeline_grid_secondary),
                );
            }
        }
    }

    fn draw_automation(
        &self,
        painter: &egui::Painter,
        palette: &HarmoniqPalette,
        track: &SequencerTrack,
        row_rect: Rect,
        beat_width: f32,
    ) {
        for lane in &track.automation {
            if !lane.visible || lane.points.len() < 2 {
                continue;
            }
            let lane_rect = Rect::from_min_max(
                Pos2::new(row_rect.left(), row_rect.bottom() - 28.0),
                Pos2::new(row_rect.right(), row_rect.bottom() - 8.0),
            );
            painter.rect_filled(lane_rect, 6.0, palette.panel_alt.linear_multiply(0.6));
            painter.rect_stroke(
                lane_rect,
                6.0,
                Stroke::new(1.0, palette.timeline_grid_secondary),
            );

            let mut points: Vec<Pos2> = lane
                .points
                .iter()
                .map(|point| {
                    let x = lane_rect.left() + point.position * beat_width;
                    let y = lane_rect.bottom() - (lane_rect.height() * point.value.clamp(0.0, 1.0));
                    Pos2::new(x, y)
                })
                .collect();
            if points.len() >= 2 {
                painter.add(Shape::line(points.clone(), Stroke::new(1.6, track.color)));
                for point in points.drain(..) {
                    painter.circle_filled(point, 3.5, track.color);
                }
            }

            painter.text(
                Pos2::new(lane_rect.left() + 8.0, lane_rect.top() - 4.0),
                Align2::LEFT_BOTTOM,
                &lane.parameter,
                FontId::proportional(12.0),
                palette.text_muted,
            );
        }
    }

    fn handle_pointer_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        track_list_rect: Rect,
        arrangement_rect: Rect,
        beat_width: f32,
        beats_per_bar: f32,
        total_beats: f32,
        event_bus: &EventBus,
    ) {
        let pointer = response.interact_pointer_pos();

        if response.clicked_by(PointerButton::Primary) {
            if let Some(pointer) = pointer {
                if track_list_rect.contains(pointer) {
                    let track_index =
                        ((pointer.y - track_list_rect.top()) / TRACK_ROW_HEIGHT).floor() as usize;
                    if track_index < self.tracks.len() {
                        let (solo_rect, mute_rect) =
                            self.button_hit_rects(track_index, &track_list_rect);
                        if solo_rect.contains(pointer) {
                            self.toggle_solo(track_index);
                        } else if mute_rect.contains(pointer) {
                            self.toggle_mute(track_index);
                        } else {
                            self.selected_track = Some(track_index);
                        }
                    }
                } else if arrangement_rect.contains(pointer) {
                    let track_index =
                        ((pointer.y - arrangement_rect.top()) / TRACK_ROW_HEIGHT).floor() as usize;
                    if track_index < self.tracks.len() {
                        let lane_rect =
                            self.automation_rect_for_track(track_index, arrangement_rect);
                        if ui.input(|i| i.modifiers.alt) && lane_rect.contains(pointer) {
                            self.insert_automation_point(
                                track_index,
                                0,
                                pointer,
                                lane_rect,
                                beat_width,
                            );
                        } else if let Some((lane_index, point_index)) = self
                            .hit_test_automation_point(
                                track_index,
                                pointer,
                                arrangement_rect,
                                beat_width,
                            )
                        {
                            self.dragging_automation = Some(AutomationDragState {
                                track_index,
                                lane_index,
                                point_index,
                            });
                        } else if let Some(clip_index) =
                            self.hit_test_clip(track_index, pointer, arrangement_rect, beat_width)
                        {
                            self.selected_track = Some(track_index);
                            self.selected_clip = Some((track_index, clip_index));
                            if response.double_clicked() {
                                self.open_clip_pattern(track_index, clip_index, event_bus);
                            } else {
                                let clip = self.tracks[track_index].clips[clip_index].clone();
                                let grab_offset = ((pointer.x - arrangement_rect.left())
                                    / beat_width)
                                    - clip.start;
                                self.dragging_clip = Some(ClipDragState {
                                    track_index,
                                    clip_id: clip.id,
                                    grab_offset_beats: grab_offset,
                                });
                            }
                        } else {
                            self.selected_clip = None;
                            self.selected_track = Some(track_index);
                            let beat = ((pointer.x - arrangement_rect.left()) / beat_width)
                                .clamp(0.0, total_beats);
                            self.pending_context_menu = Some(ContextMenuRequest {
                                track_index,
                                beat,
                                pointer,
                            });
                        }
                    } else {
                        self.selected_clip = None;
                    }
                } else {
                    self.selected_clip = None;
                }
            }
        }

        if let Some(drag_state) = self.dragging_clip {
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                let mut new_start = ((pointer.x - arrangement_rect.left()) / beat_width)
                    - drag_state.grab_offset_beats;
                new_start = new_start.max(0.0);
                if self.snap_enabled {
                    new_start = self.snap_to_grid(new_start, beats_per_bar);
                }
                self.move_clip_to(
                    drag_state.track_index,
                    drag_state.clip_id,
                    new_start,
                    total_beats,
                );
            }
        }

        if response.dragged_stopped_by(PointerButton::Primary) {
            self.dragging_clip = None;
            self.dragging_automation = None;
        }

        if let Some(drag_state) = self.dragging_automation {
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                self.update_automation_point(
                    drag_state.track_index,
                    drag_state.lane_index,
                    drag_state.point_index,
                    pointer,
                    arrangement_rect,
                    beat_width,
                );
            }
        }

        if response.clicked_by(PointerButton::Secondary) {
            if let Some(pointer) = pointer {
                if arrangement_rect.contains(pointer) {
                    let beat = ((pointer.x - arrangement_rect.left()) / beat_width)
                        .clamp(0.0, total_beats);
                    let track_index =
                        ((pointer.y - arrangement_rect.top()) / TRACK_ROW_HEIGHT).floor() as usize;
                    if track_index < self.tracks.len() {
                        self.pending_context_menu = Some(ContextMenuRequest {
                            track_index,
                            beat,
                            pointer,
                        });
                    }

                    let length = self
                        .loop_region
                        .map(|(start, end)| (end - start).max(1.0))
                        .unwrap_or(4.0);
                    self.loop_region = Some((beat, (beat + length).min(total_beats)));
                }
            }
        }
    }

    fn show_context_menu(&mut self, ui: &egui::Ui, beats_per_bar: f32, total_beats: f32) {
        if let Some(request) = self.pending_context_menu.take() {
            let mut open = true;
            egui::Window::new("Arrangement Context Menu")
                .open(&mut open)
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .frame(egui::Frame::menu(ui.style()))
                .anchor(Align2::LEFT_TOP, [0.0, 0.0])
                .fixed_pos(request.pointer)
                .show(ui.ctx(), |ui| {
                    if ui
                        .button("Add Clip (1 bar)")
                        .on_hover_text("Insert a new clip at the clicked position")
                        .clicked()
                    {
                        self.insert_clip(request.track_index, request.beat, beats_per_bar);
                        self.pending_context_menu = None;
                    }
                    if ui
                        .button("Set Loop From Here")
                        .on_hover_text("Move the loop to start from this beat")
                        .clicked()
                    {
                        let length = self
                            .loop_region
                            .map(|(start, end)| (end - start).max(1.0))
                            .unwrap_or(beats_per_bar);
                        self.loop_region =
                            Some((request.beat, (request.beat + length).min(total_beats)));
                        self.pending_context_menu = None;
                    }
                    ui.separator();
                    if ui
                        .button("Cancel")
                        .on_hover_text("Dismiss the menu without changes")
                        .clicked()
                    {
                        self.pending_context_menu = None;
                    }
                });
            if !open {
                self.pending_context_menu = None;
            }
        }
    }

    fn automation_rect_for_track(&self, track_index: usize, arrangement_rect: Rect) -> Rect {
        let top = arrangement_rect.top() + track_index as f32 * TRACK_ROW_HEIGHT;
        let row_rect = Rect::from_min_max(
            Pos2::new(arrangement_rect.left(), top),
            Pos2::new(arrangement_rect.right(), top + TRACK_ROW_HEIGHT),
        );
        Rect::from_min_max(
            Pos2::new(row_rect.left(), row_rect.bottom() - 28.0),
            Pos2::new(row_rect.right(), row_rect.bottom() - 8.0),
        )
    }

    fn insert_clip(&mut self, track_index: usize, beat: f32, beats_per_bar: f32) {
        let clip_id = self.allocate_clip_id();

        if let Some(track) = self.tracks.get_mut(track_index) {
            let pattern_id = self
                .pattern_library
                .ensure_blank(format!("Pattern {}", track.clips.len() + 1));
            let clip = SequencerClip {
                id: clip_id,
                start: beat,
                length: beats_per_bar,
                name: "New Clip".into(),
                color: track.color,
                pattern_id,
            };
            track.clips.push(clip);
            track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
            self.selected_clip = track
                .clips
                .iter()
                .position(|c| c.id == clip_id)
                .map(|index| (track_index, index));
        }
    }

    fn move_clip_to(&mut self, track_index: usize, clip_id: u64, new_start: f32, total_beats: f32) {
        if let Some(track) = self.tracks.get_mut(track_index) {
            if let Some(clip) = track.clips.iter_mut().find(|clip| clip.id == clip_id) {
                clip.start = new_start.min((total_beats - clip.length).max(0.0));
            }
            track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
            if let Some((track_sel, _)) = self.selected_clip {
                if track_sel == track_index {
                    if let Some(new_index) = track.clips.iter().position(|c| c.id == clip_id) {
                        self.selected_clip = Some((track_index, new_index));
                    }
                }
            }
        }
    }

    fn snap_to_grid(&self, value: f32, beats_per_bar: f32) -> f32 {
        let resolution = if self.show_sub_beats { 0.25 } else { 1.0 };
        let snapped = (value / resolution).round() * resolution;
        snapped.clamp(0.0, self.total_bars as f32 * beats_per_bar)
    }

    fn insert_automation_point(
        &mut self,
        track_index: usize,
        lane_index: usize,
        pointer: Pos2,
        lane_rect: Rect,
        beat_width: f32,
    ) {
        if let Some(track) = self.tracks.get_mut(track_index) {
            if let Some(lane) = track.automation.get_mut(lane_index) {
                let position = ((pointer.x - lane_rect.left()) / beat_width).max(0.0);
                let value = ((lane_rect.bottom() - pointer.y) / lane_rect.height()).clamp(0.0, 1.0);
                lane.points.push(AutomationPoint { position, value });
                lane.points
                    .sort_by(|a, b| a.position.total_cmp(&b.position));
            }
        }
    }

    fn hit_test_automation_point(
        &self,
        track_index: usize,
        pointer: Pos2,
        arrangement_rect: Rect,
        beat_width: f32,
    ) -> Option<(usize, usize)> {
        let lane_rect = self.automation_rect_for_track(track_index, arrangement_rect);
        let radius = 8.0;
        if !lane_rect.contains(pointer) {
            return None;
        }
        let track = self.tracks.get(track_index)?;
        for (lane_index, lane) in track.automation.iter().enumerate() {
            for (point_index, point) in lane.points.iter().enumerate() {
                let x = lane_rect.left() + point.position * beat_width;
                let y = lane_rect.bottom() - (lane_rect.height() * point.value);
                let distance = (pointer - Pos2::new(x, y)).length();
                if distance <= radius {
                    return Some((lane_index, point_index));
                }
            }
        }
        None
    }

    fn update_automation_point(
        &mut self,
        track_index: usize,
        lane_index: usize,
        point_index: usize,
        pointer: Pos2,
        arrangement_rect: Rect,
        beat_width: f32,
    ) {
        let lane_rect = self.automation_rect_for_track(track_index, arrangement_rect);
        if let Some(track) = self.tracks.get_mut(track_index) {
            if let Some(lane) = track.automation.get_mut(lane_index) {
                if let Some(point) = lane.points.get_mut(point_index) {
                    point.position = ((pointer.x - lane_rect.left()) / beat_width).max(0.0);
                    point.value =
                        ((lane_rect.bottom() - pointer.y) / lane_rect.height()).clamp(0.0, 1.0);
                }
                lane.points
                    .sort_by(|a, b| a.position.total_cmp(&b.position));
            }
        }
    }

    fn open_clip_pattern(&mut self, track_index: usize, clip_index: usize, event_bus: &EventBus) {
        if let Some(track) = self.tracks.get(track_index) {
            if let Some(clip) = track.clips.get(clip_index) {
                event_bus.publish(AppEvent::OpenPianoRollPattern {
                    pattern_id: clip.pattern_id,
                    clip_name: clip.name.clone(),
                });
            }
        }
    }

    fn toggle_mute(&mut self, track_index: usize) {
        if let Some(track) = self.tracks.get_mut(track_index) {
            track.muted = !track.muted;
            if track.muted {
                track.solo = false;
            }
        }
    }

    fn toggle_solo(&mut self, track_index: usize) {
        if let Some(track) = self.tracks.get_mut(track_index) {
            track.solo = !track.solo;
            if track.solo {
                track.muted = false;
            }
        }
    }

    fn hit_test_clip(
        &self,
        track_index: usize,
        pointer: Pos2,
        arrangement_rect: Rect,
        beat_width: f32,
    ) -> Option<usize> {
        let track = self.tracks.get(track_index)?;
        for (clip_index, clip) in track.clips.iter().enumerate() {
            let left = arrangement_rect.left() + clip.start * beat_width + CLIP_PADDING;
            let right = arrangement_rect.left() + clip.end() * beat_width - CLIP_PADDING;
            let top = arrangement_rect.top() + track_index as f32 * TRACK_ROW_HEIGHT + 12.0;
            let rect = Rect::from_min_max(
                Pos2::new(left, top),
                Pos2::new(right.max(left + 24.0), top + TRACK_ROW_HEIGHT - 24.0),
            );
            if rect.contains(pointer) {
                return Some(clip_index);
            }
        }
        None
    }
}

fn track_button_rects(header_rect: &Rect) -> (Rect, Rect) {
    let button_width = 28.0;
    let button_height = 22.0;
    let top = header_rect.top() + 10.0;
    let solo_rect = Rect::from_min_max(
        Pos2::new(header_rect.right() - 2.0 * button_width - 20.0, top),
        Pos2::new(
            header_rect.right() - button_width - 20.0,
            top + button_height,
        ),
    );
    let mute_rect = Rect::from_min_max(
        Pos2::new(header_rect.right() - button_width - 16.0, top),
        Pos2::new(header_rect.right() - 16.0, top + button_height),
    );
    (solo_rect, mute_rect)
}

fn stock_arrangements() -> Vec<StockArrangement> {
    vec![
        StockArrangement {
            name: "Sunrise Reverie".into(),
            tempo: 92,
            bars: 8,
            description:
                "Dreamy electric piano chords paired with warm analog bass and brushed drums.".into(),
            tags: vec!["Chill".into(), "Keys".into(), "Bass".into()],
        },
        StockArrangement {
            name: "Midnight Drive".into(),
            tempo: 108,
            bars: 8,
            description:
                "Tight synth bass groove, syncopated plucks and dusty drum textures for late-night vibes."
                    .into(),
            tags: vec!["Synthwave".into(), "Groove".into()],
        },
        StockArrangement {
            name: "Neon Skies".into(),
            tempo: 122,
            bars: 16,
            description: "Pulsing arps and side-chained pads designed to drop straight into uplifting house."
                .into(),
            tags: vec!["Dance".into(), "Arp".into(), "Pad".into()],
        },
        StockArrangement {
            name: "Lo-Fi Sketchbook".into(),
            tempo: 74,
            bars: 8,
            description:
                "Crackling vinyl layers, swung drum loops and lazy guitar chops made for hip-hop sketches."
                    .into(),
            tags: vec!["Lo-Fi".into(), "Guitar".into()],
        },
        StockArrangement {
            name: "Festival Sparks".into(),
            tempo: 128,
            bars: 16,
            description: "Big-room supersaws, risers and percussive drops primed for instant festival energy."
                .into(),
            tags: vec!["EDM".into(), "Supersaw".into(), "Riser".into()],
        },
    ]
}

fn demo_tracks(
    pattern_library: &mut PatternLibrary,
    next_clip_id: &mut u64,
) -> Vec<SequencerTrack> {
    let mut clip_id = *next_clip_id;
    let mut take_clip_id = || {
        let id = clip_id;
        clip_id += 1;
        id
    };

    let intro_pattern = pattern_library.register_pattern(PianoRollPattern {
        id: 1,
        name: "Intro Keys".into(),
        notes: vec![
            PianoRollNote {
                start: 0.0,
                length: 1.0,
                pitch: 60,
                velocity: 0.9,
            },
            PianoRollNote {
                start: 1.5,
                length: 0.5,
                pitch: 64,
                velocity: 0.6,
            },
            PianoRollNote {
                start: 2.0,
                length: 1.0,
                pitch: 67,
                velocity: 0.7,
            },
            PianoRollNote {
                start: 3.0,
                length: 0.75,
                pitch: 72,
                velocity: 0.8,
            },
        ],
    });
    let verse_pattern = pattern_library.register_pattern(PianoRollPattern {
        id: 2,
        name: "Verse Keys".into(),
        notes: vec![
            PianoRollNote {
                start: 0.0,
                length: 0.75,
                pitch: 62,
                velocity: 0.82,
            },
            PianoRollNote {
                start: 0.75,
                length: 0.5,
                pitch: 65,
                velocity: 0.7,
            },
            PianoRollNote {
                start: 1.25,
                length: 0.75,
                pitch: 69,
                velocity: 0.76,
            },
            PianoRollNote {
                start: 3.0,
                length: 0.5,
                pitch: 72,
                velocity: 0.78,
            },
        ],
    });
    let bass_pattern = pattern_library.register_pattern(PianoRollPattern {
        id: 3,
        name: "Bassline".into(),
        notes: vec![
            PianoRollNote {
                start: 0.0,
                length: 1.0,
                pitch: 36,
                velocity: 0.9,
            },
            PianoRollNote {
                start: 1.0,
                length: 1.0,
                pitch: 38,
                velocity: 0.82,
            },
            PianoRollNote {
                start: 2.0,
                length: 1.0,
                pitch: 41,
                velocity: 0.86,
            },
            PianoRollNote {
                start: 3.0,
                length: 1.0,
                pitch: 43,
                velocity: 0.84,
            },
        ],
    });
    let drums_pattern = pattern_library.register_pattern(PianoRollPattern {
        id: 4,
        name: "Drum Groove".into(),
        notes: vec![
            PianoRollNote {
                start: 0.0,
                length: 0.5,
                pitch: 36,
                velocity: 0.9,
            },
            PianoRollNote {
                start: 1.0,
                length: 0.5,
                pitch: 38,
                velocity: 0.8,
            },
            PianoRollNote {
                start: 2.0,
                length: 0.5,
                pitch: 36,
                velocity: 0.9,
            },
            PianoRollNote {
                start: 3.0,
                length: 0.5,
                pitch: 43,
                velocity: 0.75,
            },
        ],
    });
    let snare_fill_pattern = pattern_library.register_pattern(PianoRollPattern {
        id: 5,
        name: "Snare Fill".into(),
        notes: vec![
            PianoRollNote {
                start: 0.5,
                length: 0.25,
                pitch: 38,
                velocity: 0.78,
            },
            PianoRollNote {
                start: 1.0,
                length: 0.25,
                pitch: 40,
                velocity: 0.8,
            },
            PianoRollNote {
                start: 1.5,
                length: 0.5,
                pitch: 38,
                velocity: 0.82,
            },
        ],
    });
    let hat_loop_pattern = pattern_library.register_pattern(PianoRollPattern {
        id: 6,
        name: "Hat Loop".into(),
        notes: vec![
            PianoRollNote {
                start: 0.0,
                length: 0.25,
                pitch: 44,
                velocity: 0.62,
            },
            PianoRollNote {
                start: 0.5,
                length: 0.25,
                pitch: 46,
                velocity: 0.58,
            },
            PianoRollNote {
                start: 1.0,
                length: 0.25,
                pitch: 44,
                velocity: 0.6,
            },
            PianoRollNote {
                start: 1.5,
                length: 0.25,
                pitch: 46,
                velocity: 0.6,
            },
        ],
    });

    let tracks = vec![
        SequencerTrack {
            name: "Dream Piano".into(),
            color: Color32::from_rgb(82, 170, 255),
            clips: vec![
                SequencerClip {
                    id: take_clip_id(),
                    start: 0.0,
                    length: 4.0,
                    name: "Intro Keys".into(),
                    color: Color32::from_rgb(74, 140, 230),
                    pattern_id: intro_pattern,
                },
                SequencerClip {
                    id: take_clip_id(),
                    start: 4.0,
                    length: 4.0,
                    name: "Verse Keys".into(),
                    color: Color32::from_rgb(88, 155, 240),
                    pattern_id: verse_pattern,
                },
            ],
            automation: vec![AutomationLane {
                parameter: "Filter Cutoff".into(),
                visible: true,
                points: vec![
                    AutomationPoint {
                        position: 0.0,
                        value: 0.35,
                    },
                    AutomationPoint {
                        position: 2.0,
                        value: 0.6,
                    },
                    AutomationPoint {
                        position: 4.0,
                        value: 0.25,
                    },
                    AutomationPoint {
                        position: 7.5,
                        value: 0.8,
                    },
                ],
            }],
            muted: false,
            solo: false,
        },
        SequencerTrack {
            name: "Analog Bass".into(),
            color: Color32::from_rgb(244, 133, 101),
            clips: vec![SequencerClip {
                id: take_clip_id(),
                start: 0.0,
                length: 8.0,
                name: "Bassline".into(),
                color: Color32::from_rgb(230, 110, 82),
                pattern_id: bass_pattern,
            }],
            automation: vec![AutomationLane {
                parameter: "Drive".into(),
                visible: true,
                points: vec![
                    AutomationPoint {
                        position: 0.0,
                        value: 0.2,
                    },
                    AutomationPoint {
                        position: 3.0,
                        value: 0.65,
                    },
                    AutomationPoint {
                        position: 5.5,
                        value: 0.4,
                    },
                    AutomationPoint {
                        position: 7.5,
                        value: 0.7,
                    },
                ],
            }],
            muted: false,
            solo: false,
        },
        SequencerTrack {
            name: "Drums".into(),
            color: Color32::from_rgb(80, 224, 190),
            clips: vec![
                SequencerClip {
                    id: take_clip_id(),
                    start: 0.0,
                    length: 2.0,
                    name: "Hat Loop".into(),
                    color: Color32::from_rgb(70, 210, 175),
                    pattern_id: hat_loop_pattern,
                },
                SequencerClip {
                    id: take_clip_id(),
                    start: 2.0,
                    length: 2.0,
                    name: "Snare Fill".into(),
                    color: Color32::from_rgb(65, 200, 166),
                    pattern_id: snare_fill_pattern,
                },
                SequencerClip {
                    id: take_clip_id(),
                    start: 4.0,
                    length: 4.0,
                    name: "Full Kit".into(),
                    color: Color32::from_rgb(75, 215, 180),
                    pattern_id: drums_pattern,
                },
            ],
            automation: vec![AutomationLane {
                parameter: "Room Send".into(),
                visible: true,
                points: vec![
                    AutomationPoint {
                        position: 0.0,
                        value: 0.15,
                    },
                    AutomationPoint {
                        position: 3.5,
                        value: 0.35,
                    },
                    AutomationPoint {
                        position: 6.0,
                        value: 0.55,
                    },
                    AutomationPoint {
                        position: 7.5,
                        value: 0.2,
                    },
                ],
            }],
            muted: false,
            solo: false,
        },
    ];

    *next_clip_id = clip_id;
    tracks
}
