use std::path::PathBuf;

use egui::{
    Align, Align2, Button, Color32, Id, Layout, Painter, Pos2, Rect, Response, RichText, Sense,
    Stroke, TextStyle, Ui, Vec2,
};

use crate::state::{
    Clip, ClipId, ClipKind, Playlist, PlaylistClipKind, RackSlotKind, Snap, Track,
    TrackId as StateTrackId,
};

const TRACK_HEADER_HEIGHT: f32 = 68.0;
const LANE_HEIGHT: f32 = 54.0;
const TRACK_GAP: f32 = 14.0;
const INSPECTOR_WIDTH: f32 = 240.0;
const RACK_WIDTH: f32 = 190.0;
const COLUMN_GAP: f32 = 16.0;
const RULER_HEIGHT: f32 = 32.0;
const MIN_BEATS: f32 = 8.0;
const BEAT_WIDTH: f32 = 72.0;
const RACK_SLOT_HEIGHT: f32 = 34.0;
const RACK_SLOT_GAP: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackId(pub u32);

impl From<StateTrackId> for TrackId {
    fn from(value: StateTrackId) -> Self {
        Self(value.0)
    }
}

impl From<TrackId> for StateTrackId {
    fn from(value: TrackId) -> Self {
        StateTrackId(value.0)
    }
}

pub struct PlaylistProps<'a> {
    pub playlist: &'a mut Playlist,
    pub current_time_ticks: u64,
    pub snap: &'a mut Snap,
    pub open_piano_roll: &'a mut dyn FnMut(TrackId, Option<u32>, ClipId),
    pub pick_pattern_id: &'a mut dyn FnMut() -> Option<u32>,
    pub import_audio_file: &'a mut dyn FnMut(PathBuf) -> Option<crate::state::ImportedAudioSource>,
}

pub fn render(ui: &mut Ui, mut props: PlaylistProps<'_>) {
    let ppq = props.playlist.ppq().max(1) as f32;
    let ppq_ticks = props.playlist.ppq().max(1) as u64;
    let total_beats = props
        .playlist
        .tracks
        .iter()
        .flat_map(|track| track.lanes.iter().flat_map(|lane| lane.clips.iter()))
        .map(|clip| clip.end_ticks() as f32 / ppq)
        .fold(MIN_BEATS, f32::max);
    let timeline_width = total_beats.max(MIN_BEATS) * BEAT_WIDTH;
    let tracks_height = playlist_tracks_height(&props.playlist.tracks);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Snap");
            egui::ComboBox::from_id_source("playlist_snap")
                .selected_text(props.snap.label())
                .show_ui(ui, |ui| {
                    for snap in [
                        Snap::N1_1,
                        Snap::N1_2,
                        Snap::N1_4,
                        Snap::N1_8,
                        Snap::N1_16,
                        Snap::N1_32,
                        Snap::N1_64,
                    ] {
                        ui.selectable_value(props.snap, snap, snap.label());
                    }
                });
        });
        ui.add_space(8.0);

        let scroll = egui::ScrollArea::both()
            .id_source("playlist_scroll_area")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let desired_size = Vec2::new(
                    INSPECTOR_WIDTH + RACK_WIDTH + timeline_width + COLUMN_GAP * 4.0 + 160.0,
                    RULER_HEIGHT + tracks_height + 220.0,
                );
                let (container_rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
                let rect = container_rect.shrink2(Vec2::new(72.0, 48.0));

                let inspector_rect = Rect::from_min_max(
                    rect.min,
                    Pos2::new(rect.left() + INSPECTOR_WIDTH, rect.bottom()),
                );
                let rack_rect = Rect::from_min_max(
                    Pos2::new(inspector_rect.right() + COLUMN_GAP, rect.top()),
                    Pos2::new(
                        inspector_rect.right() + COLUMN_GAP + RACK_WIDTH,
                        rect.bottom(),
                    ),
                );
                let timeline_rect = Rect::from_min_max(
                    Pos2::new(rack_rect.right() + COLUMN_GAP, rect.top()),
                    rect.max,
                );

                let layout_start = rect.top() + RULER_HEIGHT + 18.0;
                let layouts = compute_vertical_layout(&props.playlist.tracks, layout_start);
                let lanes_rect = Rect::from_min_max(
                    Pos2::new(timeline_rect.left(), layout_start),
                    Pos2::new(timeline_rect.right(), layout_start + tracks_height),
                );

                let painter = ui.painter();
                painter.rect_filled(rect, 18.0, Color32::from_rgb(12, 12, 12));
                painter.rect_filled(inspector_rect, 14.0, Color32::from_rgb(26, 26, 26));
                painter.rect_filled(rack_rect, 14.0, Color32::from_rgb(30, 30, 30));
                painter.rect_filled(timeline_rect, 14.0, Color32::from_rgb(18, 18, 18));

                let header_rect = Rect::from_min_max(
                    Pos2::new(inspector_rect.left(), rect.top()),
                    Pos2::new(timeline_rect.right(), rect.top() + RULER_HEIGHT),
                );
                painter.rect_filled(header_rect, 14.0, Color32::from_rgb(40, 40, 40));

                draw_header_label(ui, inspector_rect, "Track Inspector");
                draw_header_label(ui, rack_rect, "Rack");
                draw_header_label(ui, timeline_rect, "Playlist");

                draw_inspector(ui, inspector_rect, &layouts, &mut props.playlist.tracks);
                draw_rack(ui, rack_rect, &layouts, &mut props.playlist.tracks);

                let timeline_id = Id::new("playlist_timeline_area");
                let timeline_response =
                    ui.interact(timeline_rect, timeline_id, Sense::click_and_drag());
                let timeline_painter = ui.painter().with_clip_rect(timeline_rect);
                let selection = props
                    .playlist
                    .selection
                    .map(|(track, clip)| (TrackId(track.0), clip));
                let playlist_ppq = props.playlist.ppq();
                let click = draw_timeline(
                    ui,
                    &timeline_painter,
                    &timeline_response,
                    timeline_rect,
                    lanes_rect,
                    &layouts,
                    &mut props.playlist.tracks,
                    total_beats,
                    selection,
                    playlist_ppq,
                    props.current_time_ticks,
                    *props.snap,
                );

                if click.clicked_clip.is_some() {
                    if let Some((track_id, clip_id)) = click.clicked_clip {
                        props
                            .playlist
                            .set_selection(StateTrackId::from(track_id), clip_id);
                    }
                } else if timeline_response.clicked() {
                    if let Some((track_id, clip_id)) = click.clicked_clip {
                        props
                            .playlist
                            .set_selection(StateTrackId::from(track_id), clip_id);
                    } else {
                        props.playlist.clear_selection();
                    }
                }

                if timeline_response.double_clicked() {
                    if let Some((track_id, clip_id)) = click.clicked_clip {
                        let pattern = props
                            .playlist
                            .clip(StateTrackId::from(track_id), clip_id)
                            .and_then(|clip| match clip.kind {
                                ClipKind::Pattern { pattern_id } => Some(pattern_id),
                                _ => None,
                            });
                        (props.open_piano_roll)(track_id, pattern, clip_id);
                    } else if let Some((track_id, lane_id)) = click.target_lane {
                        if let Some(pointer_pos) = timeline_response.interact_pointer_pos() {
                            let beat = ((pointer_pos.x - lanes_rect.left()) / BEAT_WIDTH).max(0.0);
                            let snap = props.snap.division() as f32;
                            let beat = (beat * snap).round() / snap;
                            let ticks = (beat * ppq) as u64;
                            let state_track_id: StateTrackId = track_id.into();
                            if let Some(track) = props
                                .playlist
                                .tracks
                                .iter_mut()
                                .find(|track| track.id == state_track_id)
                            {
                                if let Some(pattern_id) = (props.pick_pattern_id)() {
                                    let mut clip = Clip::new(
                                        ClipId(rand::random::<u64>()),
                                        "Pattern Clip",
                                        ticks,
                                        ppq_ticks.max(1),
                                        track.color,
                                        crate::state::ClipKind::Pattern { pattern_id },
                                    );
                                    clip.fade_in_ticks = 0;
                                    clip.fade_out_ticks = 0;
                                    track.add_clip_to_lane(lane_id, clip);
                                }
                            }
                        }
                    }
                }

                handle_file_drop(
                    ui,
                    timeline_rect,
                    lanes_rect,
                    &layouts,
                    props.playlist,
                    *props.snap,
                    &mut props.import_audio_file,
                );

                timeline_response
            });

        scroll
    });
}

struct ClipClickInfo {
    clicked_clip: Option<(TrackId, ClipId)>,
    target_lane: Option<(TrackId, u32)>,
}

fn handle_file_drop(
    ui: &Ui,
    timeline_rect: Rect,
    lanes_rect: Rect,
    layouts: &[TrackVerticalLayout],
    playlist: &mut Playlist,
    snap: Snap,
    import_audio: &mut dyn FnMut(PathBuf) -> Option<crate::state::ImportedAudioSource>,
) {
    let (dropped, pointer_pos) = ui.input(|i| (i.raw.dropped_files.clone(), i.pointer.hover_pos()));
    let Some(pos) = pointer_pos else {
        return;
    };

    for file in dropped {
        let Some(path) = file.path.clone() else {
            continue;
        };

        if !timeline_rect.contains(pos) {
            continue;
        }

        let Some((track_index, lane_id)) = lane_from_position(layouts, playlist, lanes_rect, pos)
        else {
            continue;
        };

        let Some(imported) = import_audio(path.clone()) else {
            continue;
        };

        let beat = ((pos.x - lanes_rect.left()) / BEAT_WIDTH).max(0.0);
        let snap_division = snap.division() as f32;
        let snapped_beats = (beat * snap_division).round() / snap_division;
        let start_ticks = (snapped_beats * playlist.ppq() as f32).round().max(0.0) as u64;
        if let Some(track) = playlist.tracks.get_mut(track_index) {
            track.ensure_audio_routing();
            let track_id = track.id;
            let track_color = track.color;

            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Audio");
            playlist.drop_clip_from_browser(
                track_id,
                lane_id,
                name,
                start_ticks,
                imported.duration_ticks.max(1),
                track_color,
                ClipKind::Audio {
                    source: imported.id,
                },
            );
        }
    }
}

fn lane_from_position(
    layouts: &[TrackVerticalLayout],
    playlist: &Playlist,
    lanes_rect: Rect,
    pos: Pos2,
) -> Option<(usize, u32)> {
    for layout in layouts {
        let Some(track) = playlist.tracks.get(layout.track_index) else {
            continue;
        };

        for lane_row in &layout.lane_rows {
            let lane_rect = Rect::from_min_max(
                Pos2::new(lanes_rect.left(), lane_row.top),
                Pos2::new(lanes_rect.right(), lane_row.bottom),
            );

            if lane_rect.contains(pos) {
                let lane = track.lanes.get(lane_row.lane_index)?;
                return Some((layout.track_index, lane.id));
            }
        }
    }

    None
}

#[derive(Debug, Clone, Copy)]
struct ClipDragState {
    track: TrackId,
    lane_id: u32,
    clip: ClipId,
    kind: DragKind,
    start_pointer: Pos2,
    original_start: u64,
    original_duration: u64,
}

impl ClipDragState {
    fn new(
        track: TrackId,
        lane_id: u32,
        clip: ClipId,
        kind: DragKind,
        start_pointer: Pos2,
        original_start: u64,
        original_duration: u64,
    ) -> Self {
        Self {
            track,
            lane_id,
            clip,
            kind,
            start_pointer,
            original_start,
            original_duration,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DragKind {
    Move,
    ResizeStart,
    ResizeEnd,
}

fn playlist_tracks_height(tracks: &[Track]) -> f32 {
    let mut height = 0.0;
    for (index, track) in tracks.iter().enumerate() {
        height += TRACK_HEADER_HEIGHT;
        height += track.lanes.len() as f32 * LANE_HEIGHT;
        if index < tracks.len().saturating_sub(1) {
            height += TRACK_GAP;
        }
    }
    height
}

struct TrackVerticalLayout {
    track_index: usize,
    header_top: f32,
    header_bottom: f32,
    lane_rows: Vec<LaneRow>,
    total_bottom: f32,
}

struct LaneRow {
    lane_index: usize,
    top: f32,
    bottom: f32,
}

fn compute_vertical_layout(tracks: &[Track], start_top: f32) -> Vec<TrackVerticalLayout> {
    let mut layouts = Vec::with_capacity(tracks.len());
    let mut top = start_top;
    for (index, track) in tracks.iter().enumerate() {
        let header_top = top;
        let header_bottom = header_top + TRACK_HEADER_HEIGHT;
        let mut lane_top = header_bottom;
        let mut lane_rows = Vec::with_capacity(track.lanes.len());
        for (lane_index, _) in track.lanes.iter().enumerate() {
            let lane_bottom = lane_top + LANE_HEIGHT;
            lane_rows.push(LaneRow {
                lane_index,
                top: lane_top,
                bottom: lane_bottom,
            });
            lane_top = lane_bottom;
        }
        let total_bottom = lane_rows
            .last()
            .map(|lane| lane.bottom)
            .unwrap_or(header_bottom);
        layouts.push(TrackVerticalLayout {
            track_index: index,
            header_top,
            header_bottom,
            lane_rows,
            total_bottom,
        });
        top = total_bottom + TRACK_GAP;
    }
    layouts
}

fn draw_inspector(
    ui: &mut Ui,
    inspector_rect: Rect,
    layouts: &[TrackVerticalLayout],
    tracks: &mut [Track],
) {
    for (layout_index, layout) in layouts.iter().enumerate() {
        let track = &mut tracks[layout.track_index];
        let header_rect = Rect::from_min_max(
            Pos2::new(inspector_rect.left() + 12.0, layout.header_top + 6.0),
            Pos2::new(inspector_rect.right() - 12.0, layout.header_bottom - 6.0),
        );
        ui.painter()
            .rect_filled(header_rect, 8.0, track_color(track.color, 60, 0.6));
        ui.painter()
            .rect_stroke(header_rect, 8.0, Stroke::new(1.0, Color32::from_gray(70)));
        ui.allocate_ui_at_rect(header_rect, |ui| {
            ui.set_min_size(header_rect.size());
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&track.name)
                            .strong()
                            .color(Color32::from_rgb(240, 240, 240)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        track_toggle_button(
                            ui,
                            &mut track.controls.monitor,
                            "Mon",
                            Color32::from_rgb(105, 180, 255),
                        );
                        track_toggle_button(
                            ui,
                            &mut track.controls.mute,
                            "Mute",
                            Color32::from_rgb(255, 115, 115),
                        );
                        track_toggle_button(
                            ui,
                            &mut track.controls.solo,
                            "Solo",
                            Color32::from_rgb(255, 214, 102),
                        );
                        track_toggle_button(
                            ui,
                            &mut track.controls.record_arm,
                            "Rec",
                            Color32::from_rgb(255, 90, 90),
                        );
                    });
                });
                ui.add_space(4.0);
                let color_rect = Rect::from_min_max(
                    Pos2::new(header_rect.left() + 2.0, header_rect.bottom() - 10.0),
                    Pos2::new(header_rect.right() - 2.0, header_rect.bottom() - 4.0),
                );
                ui.painter()
                    .rect_filled(color_rect, 2.0, track_color(track.color, 120, 1.0));
            });
        });

        for lane_row in &layout.lane_rows {
            let lane_rect = Rect::from_min_max(
                Pos2::new(inspector_rect.left() + 20.0, lane_row.top + 6.0),
                Pos2::new(inspector_rect.right() - 20.0, lane_row.bottom - 6.0),
            );
            ui.painter()
                .rect_filled(lane_rect, 6.0, Color32::from_rgb(36, 36, 36));
            ui.painter()
                .rect_stroke(lane_rect, 6.0, Stroke::new(1.0, Color32::from_gray(52)));
            let lane_name = tracks[layout.track_index].lanes[lane_row.lane_index]
                .name
                .clone();
            ui.allocate_ui_at_rect(lane_rect, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(lane_name).color(Color32::from_rgb(210, 210, 210)));
                });
            });
        }

        if layout_index < layouts.len().saturating_sub(1) {
            let y = layout.total_bottom + TRACK_GAP * 0.5;
            ui.painter().line_segment(
                [
                    Pos2::new(inspector_rect.left() + 8.0, y),
                    Pos2::new(inspector_rect.right() - 8.0, y),
                ],
                Stroke::new(1.0, Color32::from_gray(44)),
            );
        }
    }
}

fn track_toggle_button(ui: &mut Ui, value: &mut bool, label: &str, on_color: Color32) {
    let button = Button::new(
        RichText::new(label)
            .size(12.0)
            .color(Color32::from_rgb(230, 230, 230)),
    )
    .frame(false);
    let resp = ui.add(button);
    if resp.clicked() {
        *value = !*value;
    }
    if *value {
        let rect = resp.rect;
        ui.painter()
            .rect_filled(rect, 6.0, on_color.gamma_multiply(0.35));
        ui.painter()
            .rect_stroke(rect, 6.0, Stroke::new(1.0, on_color.gamma_multiply(0.6)));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            TextStyle::Button.resolve(ui.style()),
            Color32::from_rgb(15, 15, 15),
        );
    }
}

fn draw_rack(ui: &mut Ui, rack_rect: Rect, layouts: &[TrackVerticalLayout], tracks: &mut [Track]) {
    for (layout_index, layout) in layouts.iter().enumerate() {
        let track = &mut tracks[layout.track_index];
        let track_rect = Rect::from_min_max(
            Pos2::new(rack_rect.left() + 12.0, layout.header_top + 6.0),
            Pos2::new(rack_rect.right() - 12.0, layout.total_bottom - 6.0),
        );
        ui.painter()
            .rect_filled(track_rect, 8.0, Color32::from_rgb(34, 34, 34));
        ui.painter()
            .rect_stroke(track_rect, 8.0, Stroke::new(1.0, Color32::from_gray(60)));

        let mut slot_top = track_rect.top() + 10.0;
        for slot in track.rack.iter_mut() {
            let slot_rect = Rect::from_min_max(
                Pos2::new(track_rect.left() + 8.0, slot_top),
                Pos2::new(track_rect.right() - 8.0, slot_top + RACK_SLOT_HEIGHT),
            );
            let base_color = match slot.kind {
                RackSlotKind::Instrument => Color32::from_rgb(66, 128, 255),
                RackSlotKind::Insert => Color32::from_rgb(120, 170, 255),
                RackSlotKind::Send => Color32::from_rgb(100, 200, 150),
                RackSlotKind::Midi => Color32::from_rgb(200, 140, 255),
            };
            let fill = if slot.active {
                base_color.gamma_multiply(0.35)
            } else {
                Color32::from_rgb(32, 32, 32)
            };
            ui.painter().rect_filled(slot_rect, 6.0, fill);
            ui.painter().rect_stroke(
                slot_rect,
                6.0,
                Stroke::new(1.0, base_color.gamma_multiply(0.7)),
            );

            let response = ui.interact(
                slot_rect,
                Id::new((track.id.0, slot.id, "rack_slot")),
                Sense::click(),
            );
            if response.clicked() {
                slot.toggle();
            }

            ui.painter().text(
                slot_rect.left_top() + Vec2::new(8.0, 6.0),
                Align2::LEFT_TOP,
                &slot.name,
                TextStyle::Body.resolve(ui.style()),
                Color32::from_rgb(235, 235, 235),
            );
            ui.painter().text(
                slot_rect.left_bottom() + Vec2::new(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                slot.kind.label(),
                TextStyle::Small.resolve(ui.style()),
                Color32::from_gray(200),
            );

            slot_top += RACK_SLOT_HEIGHT + RACK_SLOT_GAP;
        }

        if layout_index < layouts.len().saturating_sub(1) {
            let y = layout.total_bottom + TRACK_GAP * 0.5;
            ui.painter().line_segment(
                [
                    Pos2::new(rack_rect.left() + 8.0, y),
                    Pos2::new(rack_rect.right() - 8.0, y),
                ],
                Stroke::new(1.0, Color32::from_gray(44)),
            );
        }
    }
}

fn draw_timeline(
    ui: &Ui,
    painter: &Painter,
    response: &Response,
    timeline_rect: Rect,
    lanes_rect: Rect,
    layouts: &[TrackVerticalLayout],
    tracks: &mut [Track],
    total_beats: f32,
    selection: Option<(TrackId, ClipId)>,
    ppq: u32,
    current_time: u64,
    snap: Snap,
) -> ClipClickInfo {
    let ruler_rect = Rect::from_min_max(
        Pos2::new(timeline_rect.left(), timeline_rect.top()),
        Pos2::new(timeline_rect.right(), timeline_rect.top() + RULER_HEIGHT),
    );

    draw_ruler(painter, ruler_rect, total_beats);
    draw_grid(
        painter,
        Rect::from_min_max(
            Pos2::new(lanes_rect.left(), ruler_rect.bottom()),
            lanes_rect.right_bottom(),
        ),
        total_beats,
    );

    let hover_pos = response.hover_pos();
    let mut clicked_clip = None;
    let mut target_lane = None;
    let mut drag_state = ui
        .ctx()
        .data(|data| data.get_temp::<ClipDragState>(clip_drag_id(ui.ctx())));

    for layout in layouts {
        let track = &mut tracks[layout.track_index];
        let track_base_color = track_color(track.color, 40, 0.5);
        let accent_rect = Rect::from_min_max(
            Pos2::new(lanes_rect.left() - 6.0, layout.header_top),
            Pos2::new(lanes_rect.left(), layout.total_bottom),
        );
        painter.rect_filled(accent_rect, 2.0, track_base_color);

        for lane_row in &layout.lane_rows {
            let lane_rect = Rect::from_min_max(
                Pos2::new(lanes_rect.left(), lane_row.top),
                Pos2::new(lanes_rect.right(), lane_row.bottom),
            );
            let lane_bg = if let Some(pos) = hover_pos {
                if lane_rect.contains(pos) {
                    Color32::from_rgb(42, 42, 42)
                } else {
                    Color32::from_rgb(32, 32, 32)
                }
            } else {
                Color32::from_rgb(32, 32, 32)
            };
            painter.rect_filled(lane_rect, 6.0, lane_bg);
            painter.rect_stroke(
                lane_rect,
                6.0,
                Stroke::new(1.0, Color32::from_rgb(54, 54, 54)),
            );

            let lane = &mut track.lanes[lane_row.lane_index];
            let mut resort_lane = false;
            for clip_index in 0..lane.clips.len() {
                let clip = lane.clips[clip_index].clone();
                let clip_kind = clip.to_playlist_clip(layout.track_index, lane.id).kind;
                let clip_rect = clip_rect(lane_rect, &clip, ppq);
                let clip_id =
                    Id::new(("playlist_clip", track.id.0, lane_row.lane_index, clip.id.0));
                let clip_response = ui.interact(clip_rect, clip_id, Sense::click_and_drag());
                let is_selected = selection
                    .map(|(track_id, clip_id)| {
                        track_id == TrackId(track.id.0) && clip_id == clip.id
                    })
                    .unwrap_or(false);
                let fill_color = if is_selected {
                    track_color(track.color, 180, 1.15)
                } else {
                    track_color(track.color, 130, 0.95)
                };
                painter.rect_filled(clip_rect, 6.0, fill_color);
                painter.rect_stroke(
                    clip_rect,
                    6.0,
                    Stroke::new(1.0, Color32::from_gray(if is_selected { 255 } else { 80 })),
                );
                painter.text(
                    clip_rect.left_top() + Vec2::new(8.0, 6.0),
                    Align2::LEFT_TOP,
                    &clip.name,
                    TextStyle::Body.resolve(painter.ctx().style().as_ref()),
                    Color32::from_rgb(10, 10, 10),
                );

                let (badge_label, badge_color) = clip_kind_badge(&clip_kind);
                let badge_rect = Rect::from_min_max(
                    Pos2::new(clip_rect.right() - 94.0, clip_rect.top() + 6.0),
                    Pos2::new(clip_rect.right() - 10.0, clip_rect.top() + 22.0),
                );
                painter.rect_filled(badge_rect, 4.0, badge_color);
                painter.text(
                    badge_rect.center(),
                    Align2::CENTER_CENTER,
                    badge_label,
                    TextStyle::Small.resolve(painter.ctx().style().as_ref()),
                    Color32::from_rgb(8, 8, 8),
                );

                let handle_width = 8.0;
                let left_handle = Rect::from_min_max(
                    Pos2::new(clip_rect.left(), clip_rect.top()),
                    Pos2::new(clip_rect.left() + handle_width, clip_rect.bottom()),
                );
                let right_handle = Rect::from_min_max(
                    Pos2::new(clip_rect.right() - handle_width, clip_rect.top()),
                    Pos2::new(clip_rect.right(), clip_rect.bottom()),
                );

                let left_handle_resp = ui.interact(
                    left_handle,
                    Id::new((
                        "playlist_clip_left",
                        track.id.0,
                        lane_row.lane_index,
                        clip.id.0,
                    )),
                    Sense::click_and_drag(),
                );
                let right_handle_resp = ui.interact(
                    right_handle,
                    Id::new((
                        "playlist_clip_right",
                        track.id.0,
                        lane_row.lane_index,
                        clip.id.0,
                    )),
                    Sense::click_and_drag(),
                );

                if clip_response.clicked()
                    || left_handle_resp.clicked()
                    || right_handle_resp.clicked()
                {
                    clicked_clip = Some((TrackId(track.id.0), clip.id));
                }

                if left_handle_resp.drag_started() {
                    drag_state = Some(ClipDragState::new(
                        TrackId(track.id.0),
                        lane.id,
                        clip.id,
                        DragKind::ResizeStart,
                        left_handle_resp
                            .interact_pointer_pos()
                            .or_else(|| clip_response.interact_pointer_pos())
                            .unwrap_or(clip_rect.left_top()),
                        clip.start_ticks,
                        clip.duration_ticks,
                    ));
                } else if right_handle_resp.drag_started() {
                    drag_state = Some(ClipDragState::new(
                        TrackId(track.id.0),
                        lane.id,
                        clip.id,
                        DragKind::ResizeEnd,
                        right_handle_resp
                            .interact_pointer_pos()
                            .or_else(|| clip_response.interact_pointer_pos())
                            .unwrap_or(clip_rect.right_top()),
                        clip.start_ticks,
                        clip.duration_ticks,
                    ));
                } else if clip_response.drag_started() {
                    drag_state = Some(ClipDragState::new(
                        TrackId(track.id.0),
                        lane.id,
                        clip.id,
                        DragKind::Move,
                        clip_response
                            .interact_pointer_pos()
                            .unwrap_or(clip_rect.left_top()),
                        clip.start_ticks,
                        clip.duration_ticks,
                    ));
                }

                if let Some(state) = drag_state.as_ref().filter(|state| {
                    state.track == TrackId(track.id.0)
                        && state.clip == clip.id
                        && state.lane_id == lane.id
                }) {
                    if let Some(pointer_pos) = clip_response.interact_pointer_pos() {
                        let delta = pointer_pos.x - state.start_pointer.x;
                        let delta_beats = delta / BEAT_WIDTH;
                        let snapped_delta = snap_beats(delta_beats, snap, ppq);
                        match state.kind {
                            DragKind::Move => {
                                let new_start_beats =
                                    (state.original_start as f32 / ppq as f32) + snapped_delta;
                                let new_start =
                                    (new_start_beats * ppq as f32).round().max(0.0) as u64;
                                lane.clips[clip_index].start_ticks = new_start;
                                resort_lane = true;
                            }
                            DragKind::ResizeStart => {
                                let end = state.original_start + state.original_duration;
                                let new_start_beats =
                                    (state.original_start as f32 / ppq as f32) + snapped_delta;
                                let new_start =
                                    (new_start_beats * ppq as f32).round().max(0.0) as u64;
                                let new_start = new_start.min(end.saturating_sub(1));
                                lane.clips[clip_index].start_ticks = new_start;
                                lane.clips[clip_index].duration_ticks =
                                    end.saturating_sub(new_start).max(1);
                                resort_lane = true;
                            }
                            DragKind::ResizeEnd => {
                                let start = state.original_start;
                                let new_end_beats = (start as f32 / ppq as f32
                                    + state.original_duration as f32 / ppq as f32)
                                    + snapped_delta;
                                let new_end =
                                    (new_end_beats * ppq as f32).round().max(start as f32) as u64;
                                lane.clips[clip_index].duration_ticks =
                                    new_end.saturating_sub(start).max(1);
                            }
                        }
                    }

                    if clip_response.drag_released()
                        || left_handle_resp.drag_released()
                        || right_handle_resp.drag_released()
                    {
                        drag_state = None;
                    }
                }

                if response.clicked() || response.double_clicked() {
                    if let Some(pointer_pos) = response.interact_pointer_pos() {
                        if lane_rect.contains(pointer_pos) && clicked_clip.is_none() {
                            target_lane = Some((TrackId(track.id.0), lane.id));
                            if clip_rect.contains(pointer_pos) {
                                clicked_clip = Some((TrackId(track.id.0), clip.id));
                            }
                        }
                    }
                }
            }

            if response.clicked() || response.double_clicked() {
                if let Some(pointer_pos) = response.interact_pointer_pos() {
                    if lane.clips.is_empty() && lane_rect.contains(pointer_pos) {
                        target_lane = Some((TrackId(track.id.0), lane.id));
                    }
                }
            }

            if resort_lane {
                lane.clips.sort_by_key(|clip| clip.start_ticks);
            }
        }
    }

    ui.ctx()
        .data_mut(|data| data.insert_temp(clip_drag_id(ui.ctx()), drag_state));

    draw_playhead(painter, lanes_rect, current_time, ppq);

    ClipClickInfo {
        clicked_clip,
        target_lane,
    }
}

fn clip_drag_id(ctx: &egui::Context) -> Id {
    Id::new(("playlist_clip_drag", ctx.viewport_id()))
}

fn snap_beats(delta_beats: f32, snap: Snap, _ppq: u32) -> f32 {
    let division = snap.division() as f32;
    (delta_beats * division).round() / division
}

fn clip_kind_badge(kind: &PlaylistClipKind) -> (&'static str, Color32) {
    match kind {
        PlaylistClipKind::Pattern { .. } => ("Pattern", Color32::from_rgb(164, 209, 255)),
        PlaylistClipKind::Audio { .. } => ("Audio", Color32::from_rgb(255, 205, 140)),
        PlaylistClipKind::Automation { .. } => ("Automation", Color32::from_rgb(196, 164, 255)),
    }
}

fn clip_rect(lane_rect: Rect, clip: &Clip, ppq: u32) -> Rect {
    let start_beats = clip.start_ticks as f32 / ppq as f32;
    let duration_beats = clip.duration_ticks as f32 / ppq as f32;
    let left = lane_rect.left() + start_beats * BEAT_WIDTH;
    let right = left + duration_beats * BEAT_WIDTH;
    Rect::from_min_max(
        Pos2::new(left, lane_rect.top() + 4.0),
        Pos2::new(right, lane_rect.bottom() - 4.0),
    )
}

fn draw_ruler(painter: &Painter, ruler_rect: Rect, total_beats: f32) {
    painter.rect_filled(ruler_rect, 0.0, Color32::from_rgb(48, 48, 48));
    painter.rect_stroke(
        ruler_rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgb(64, 64, 64)),
    );
    let style = painter.ctx().style().clone();
    let text_style = TextStyle::Button.resolve(style.as_ref());
    let mut beat = 0;
    while (beat as f32) < total_beats + 4.0 {
        let x = ruler_rect.left() + (beat as f32 * BEAT_WIDTH);
        let color = if beat % 4 == 0 {
            Color32::from_rgb(200, 200, 200)
        } else {
            Color32::from_rgb(130, 130, 130)
        };
        painter.line_segment(
            [
                Pos2::new(x, ruler_rect.bottom()),
                Pos2::new(x, ruler_rect.bottom() - 10.0),
            ],
            Stroke::new(1.0, color),
        );
        if beat % 4 == 0 {
            painter.text(
                Pos2::new(x + 4.0, ruler_rect.center().y - 8.0),
                Align2::LEFT_CENTER,
                format!("Bar {}", beat / 4 + 1),
                text_style.clone(),
                Color32::from_rgb(240, 240, 240),
            );
        }
        beat += 1;
    }
}

fn draw_grid(painter: &Painter, rect: Rect, total_beats: f32) {
    let mut beat = 0;
    while (beat as f32) < total_beats + 4.0 {
        let x = rect.left() + (beat as f32 * BEAT_WIDTH);
        let color = if beat % 4 == 0 {
            Color32::from_rgb(70, 70, 70)
        } else {
            Color32::from_rgb(44, 44, 44)
        };
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, color),
        );
        beat += 1;
    }
}

fn draw_playhead(painter: &Painter, lanes_rect: Rect, current_time_ticks: u64, ppq: u32) {
    let ppq = ppq.max(1);
    let beat = current_time_ticks as f32 / ppq as f32;
    let x = lanes_rect.left() + beat * BEAT_WIDTH;
    let playhead_color = Color32::from_rgb(255, 120, 90);
    painter.line_segment(
        [
            Pos2::new(x, lanes_rect.top()),
            Pos2::new(x, lanes_rect.bottom()),
        ],
        Stroke::new(2.0, playhead_color),
    );
}

fn track_color(color: [f32; 4], alpha: u8, brighten: f32) -> Color32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0 * brighten).clamp(0.0, 255.0) as u8;
    let g = (color[1].clamp(0.0, 1.0) * 255.0 * brighten).clamp(0.0, 255.0) as u8;
    let b = (color[2].clamp(0.0, 1.0) * 255.0 * brighten).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

fn draw_header_label(ui: &Ui, rect: Rect, text: &str) {
    let label_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 16.0, rect.top() + 4.0),
        Pos2::new(rect.right(), rect.top() + RULER_HEIGHT - 4.0),
    );
    ui.painter().text(
        label_rect.left_top(),
        Align2::LEFT_TOP,
        text,
        TextStyle::Heading.resolve(ui.style()),
        Color32::from_rgb(230, 230, 230),
    );
}
