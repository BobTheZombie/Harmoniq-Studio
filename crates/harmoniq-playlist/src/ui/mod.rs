use std::path::PathBuf;

use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::state::{AudioSourceId, Clip, ClipId, Playlist, Snap, Track};

const TRACK_HEIGHT: f32 = 70.0;
const HEADER_WIDTH: f32 = 160.0;
const RULER_HEIGHT: f32 = 26.0;
const MIN_BEATS: f32 = 8.0;
const BEAT_WIDTH: f32 = 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackId(pub u32);

impl From<crate::state::TrackId> for TrackId {
    fn from(value: crate::state::TrackId) -> Self {
        Self(value.0)
    }
}

impl From<TrackId> for crate::state::TrackId {
    fn from(value: TrackId) -> Self {
        crate::state::TrackId(value.0)
    }
}

pub struct PlaylistProps<'a> {
    pub playlist: &'a mut Playlist,
    pub current_time_ticks: u64,
    pub snap: &'a mut Snap,
    pub open_piano_roll: &'a mut dyn FnMut(TrackId, Option<u32>, ClipId),
    pub import_audio_file: &'a mut dyn FnMut(PathBuf) -> AudioSourceId,
}

pub fn render(ui: &mut Ui, props: PlaylistProps<'_>) {
    let ppq = props.playlist.ppq() as f32;
    let ppq_ticks = props.playlist.ppq() as u64;
    let total_beats = props
        .playlist
        .tracks
        .iter()
        .flat_map(|track| track.clips.iter())
        .map(|clip| clip.end_ticks() as f32 / ppq)
        .fold(MIN_BEATS, f32::max);
    let timeline_width = total_beats.max(MIN_BEATS) * BEAT_WIDTH;
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
                    HEADER_WIDTH + timeline_width + 200.0,
                    TRACK_HEIGHT * props.playlist.tracks.len() as f32 + RULER_HEIGHT + 80.0,
                );
                let (response, painter) =
                    ui.allocate_painter(desired_size, Sense::click_and_drag());
                let rect = response.rect.shrink2(Vec2::new(60.0, 36.0));

                let header_rect = Rect::from_min_max(
                    rect.min,
                    Pos2::new(rect.left() + HEADER_WIDTH, rect.bottom()),
                );
                let ruler_rect = Rect::from_min_max(
                    Pos2::new(header_rect.right(), rect.top()),
                    Pos2::new(rect.right(), rect.top() + RULER_HEIGHT),
                );
                let timeline_rect = Rect::from_min_max(
                    Pos2::new(header_rect.right(), ruler_rect.bottom()),
                    rect.max,
                );

                painter.rect_filled(header_rect, 10.0, Color32::from_gray(26));
                painter.rect_filled(ruler_rect, 10.0, Color32::from_gray(40));
                painter.rect_filled(timeline_rect, 10.0, Color32::from_gray(18));

                draw_tracks(&painter, header_rect, &props.playlist.tracks);
                draw_ruler(&painter, ruler_rect, total_beats);
                draw_playhead(
                    &painter,
                    timeline_rect,
                    props.current_time_ticks,
                    props.playlist.ppq(),
                );
                let selection = props.playlist.selection;
                let selection = selection.map(|(track, clip)| (TrackId(track.0), clip));
                let click = draw_clips(
                    ui,
                    response.id,
                    &painter,
                    timeline_rect,
                    &props.playlist.tracks,
                    selection,
                    *props.snap,
                    props.playlist.ppq(),
                );
                if let Some((track_id, clip_id)) = click.clicked_clip {
                    props
                        .playlist
                        .set_selection(crate::state::TrackId(track_id.0), clip_id);
                }
                if click.double_clicked {
                    if let Some((track_id, clip_id)) = click.clicked_clip {
                        (props.open_piano_roll)(track_id, None, clip_id);
                    }
                }

                if let Some(pointer_pos) = response.interact_pointer_pos() {
                    if response.double_clicked() {
                        let track_index = (((pointer_pos.y - timeline_rect.top()) / TRACK_HEIGHT)
                            .floor() as usize)
                            .min(props.playlist.tracks.len().saturating_sub(1));
                        if let Some(track) = props.playlist.tracks.get_mut(track_index) {
                            let beat =
                                ((pointer_pos.x - timeline_rect.left()) / BEAT_WIDTH).max(0.0);
                            let snap = props.snap.division() as f32;
                            let beat = (beat * snap).round() / snap;
                            let ticks = (beat * ppq) as u64;
                            let clip = Clip {
                                id: ClipId(rand::random::<u64>()),
                                name: "Audio Clip".into(),
                                start_ticks: ticks,
                                duration_ticks: ppq_ticks.max(1),
                                color: track.color,
                                kind: crate::state::ClipKind::Audio {
                                    source: AudioSourceId::from_path(
                                        PathBuf::from("import.wav").as_path(),
                                    ),
                                },
                            };
                            track.add_clip(clip);
                        }
                    }
                }

                response
            });
        for dropped in props.playlist.take_dropped_files() {
            let _source_id = (props.import_audio_file)(dropped.clone());
        }
        scroll
    });
}

struct ClipClickInfo {
    clicked_clip: Option<(TrackId, ClipId)>,
    double_clicked: bool,
}

fn draw_tracks(painter: &egui::Painter, header_rect: Rect, tracks: &[Track]) {
    let mut top = header_rect.top();
    for track in tracks {
        let row_rect = Rect::from_min_max(
            Pos2::new(header_rect.left() + 12.0, top + 6.0),
            Pos2::new(header_rect.right() - 12.0, top + TRACK_HEIGHT - 6.0),
        );
        painter.rect_filled(
            row_rect,
            8.0,
            Color32::from_rgba_premultiplied(
                (track.color[0] * 255.0) as u8,
                (track.color[1] * 255.0) as u8,
                (track.color[2] * 255.0) as u8,
                32,
            ),
        );
        painter.text(
            row_rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            &track.name,
            egui::TextStyle::Button.resolve(painter.ctx().style().as_ref()),
            Color32::WHITE,
        );
        top += TRACK_HEIGHT;
    }
}

fn draw_ruler(painter: &egui::Painter, ruler_rect: Rect, total_beats: f32) {
    let mut beat = 0;
    while (beat as f32) < total_beats + 1.0 {
        let x = ruler_rect.left() + (beat as f32 * BEAT_WIDTH);
        let color = if beat % 4 == 0 {
            Color32::from_white_alpha(180)
        } else {
            Color32::from_white_alpha(90)
        };
        painter.line_segment(
            [
                Pos2::new(x, ruler_rect.top()),
                Pos2::new(x, ruler_rect.bottom()),
            ],
            Stroke::new(1.0, color),
        );
        if beat % 4 == 0 {
            painter.text(
                Pos2::new(x + 4.0, ruler_rect.center().y),
                egui::Align2::LEFT_CENTER,
                format!("Bar {}", beat / 4 + 1),
                egui::TextStyle::Body.resolve(painter.ctx().style().as_ref()),
                Color32::WHITE,
            );
        }
        beat += 1;
    }
}

fn draw_playhead(painter: &egui::Painter, timeline_rect: Rect, current_ticks: u64, ppq: u32) {
    let beats = current_ticks as f32 / ppq as f32;
    let x = timeline_rect.left() + beats * BEAT_WIDTH;
    if x >= timeline_rect.left() && x <= timeline_rect.right() {
        painter.line_segment(
            [
                Pos2::new(x, timeline_rect.top()),
                Pos2::new(x, timeline_rect.bottom()),
            ],
            Stroke::new(2.0, Color32::from_rgb(255, 100, 100)),
        );
    }
}

fn draw_clips(
    ui: &mut Ui,
    id: Id,
    painter: &egui::Painter,
    timeline_rect: Rect,
    tracks: &[Track],
    selection: Option<(TrackId, ClipId)>,
    _snap: Snap,
    ppq: u32,
) -> ClipClickInfo {
    let mut clicked: Option<(TrackId, ClipId)> = None;
    let mut double_clicked = false;
    for (index, track) in tracks.iter().enumerate() {
        for clip in &track.clips {
            let rect = clip_rect(timeline_rect, index, clip, ppq);
            let mut color = Color32::from_rgba_premultiplied(
                (clip.color[0] * 255.0) as u8,
                (clip.color[1] * 255.0) as u8,
                (clip.color[2] * 255.0) as u8,
                180,
            );
            if selection == Some((TrackId(track.id.0), clip.id)) {
                color = Color32::from_rgba_premultiplied(
                    (clip.color[0] * 255.0) as u8,
                    (clip.color[1] * 255.0) as u8,
                    (clip.color[2] * 255.0) as u8,
                    240,
                );
            }
            painter.rect_filled(rect, 10.0, color);
            painter.rect_stroke(rect, 10.0, Stroke::new(1.2, Color32::from_white_alpha(220)));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &clip.name,
                egui::TextStyle::Button.resolve(ui.style()),
                Color32::WHITE,
            );
            let response = ui.interact(rect, id.with(clip.id.0), Sense::click());
            if response.clicked() {
                clicked = Some((TrackId(track.id.0), clip.id));
            }
            if response.double_clicked() {
                clicked = Some((TrackId(track.id.0), clip.id));
                double_clicked = true;
            }
        }
    }

    ClipClickInfo {
        clicked_clip: clicked,
        double_clicked,
    }
}

fn clip_rect(timeline_rect: Rect, track_index: usize, clip: &Clip, ppq: u32) -> Rect {
    let top = timeline_rect.top() + track_index as f32 * TRACK_HEIGHT + 6.0;
    let left = timeline_rect.left() + (clip.start_ticks as f32 / ppq as f32) * BEAT_WIDTH;
    let width = (clip.duration_ticks as f32 / ppq as f32).max(1.0) * BEAT_WIDTH;
    Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, TRACK_HEIGHT - 12.0))
}
