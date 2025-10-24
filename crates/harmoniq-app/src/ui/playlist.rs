use eframe::egui::{self, Color32, RichText};
use harmoniq_engine::TransportState;
use harmoniq_ui::HarmoniqPalette;

use crate::ui::event_bus::EventBus;
use crate::TransportClock;

struct Clip {
    name: String,
    start: f32,
    length: f32,
    color: Color32,
}

struct PlaylistTrack {
    name: String,
    color: Color32,
    clips: Vec<Clip>,
}

pub struct PlaylistPane {
    tracks: Vec<PlaylistTrack>,
    playhead: f32,
    length_beats: f32,
}

impl Default for PlaylistPane {
    fn default() -> Self {
        let tracks = vec![
            PlaylistTrack {
                name: "Drums".into(),
                color: Color32::from_rgb(240, 170, 100),
                clips: vec![Clip {
                    name: "Beat".into(),
                    start: 0.0,
                    length: 8.0,
                    color: Color32::from_rgb(240, 170, 100),
                }],
            },
            PlaylistTrack {
                name: "Bass".into(),
                color: Color32::from_rgb(150, 140, 220),
                clips: vec![Clip {
                    name: "Bassline".into(),
                    start: 0.0,
                    length: 16.0,
                    color: Color32::from_rgb(150, 140, 220),
                }],
            },
            PlaylistTrack {
                name: "Lead".into(),
                color: Color32::from_rgb(130, 200, 240),
                clips: vec![Clip {
                    name: "Lead Hook".into(),
                    start: 4.0,
                    length: 8.0,
                    color: Color32::from_rgb(130, 200, 240),
                }],
            },
        ];
        Self {
            tracks,
            playhead: 0.0,
            length_beats: 32.0,
        }
    }
}

impl PlaylistPane {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        _event_bus: &EventBus,
        transport_state: TransportState,
        clock: TransportClock,
    ) {
        let _ = transport_state;

        ui.vertical(|ui| {
            ui.heading(RichText::new("Playlist").color(palette.text_primary));
            ui.label(RichText::new(format!("Clock {}", clock.format())).color(palette.text_muted));
            ui.add_space(8.0);

            let track_height = 64.0;
            let header_width = 180.0;
            let beat_width = 80.0;
            let total_height = track_height * self.tracks.len() as f32;
            let total_width = self.length_beats * beat_width;

            egui::ScrollArea::both()
                .id_source("playlist_scroll")
                .show(ui, |ui| {
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(total_width + header_width + 80.0, total_height + 80.0),
                        egui::Sense::click_and_drag(),
                    );
                    let rect = response.rect.shrink2(egui::vec2(40.0, 40.0));

                    let header_rect = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.left() + header_width, rect.bottom()),
                    );
                    let timeline_rect = egui::Rect::from_min_max(
                        egui::pos2(header_rect.right(), rect.top()),
                        rect.max,
                    );

                    painter.rect_filled(header_rect, 12.0, palette.panel_alt);
                    painter.rect_filled(timeline_rect, 12.0, palette.panel);

                    for (index, track) in self.tracks.iter().enumerate() {
                        let top = timeline_rect.top() + index as f32 * track_height;
                        let row_rect = egui::Rect::from_min_max(
                            egui::pos2(timeline_rect.left(), top),
                            egui::pos2(timeline_rect.right(), top + track_height),
                        );
                        painter.rect_filled(row_rect, 6.0, palette.panel_alt.gamma_multiply(1.05));
                        painter.rect_stroke(
                            row_rect,
                            6.0,
                            egui::Stroke::new(1.0, palette.timeline_grid_secondary),
                        );
                        painter.text(
                            egui::pos2(header_rect.left() + 18.0, top + track_height * 0.5),
                            egui::Align2::LEFT_CENTER,
                            &track.name,
                            egui::FontId::proportional(16.0),
                            track.color,
                        );
                    }

                    for beat in 0..=self.length_beats as i32 {
                        let x = timeline_rect.left() + beat as f32 * beat_width;
                        let is_bar = beat % 4 == 0;
                        let stroke = egui::Stroke::new(
                            if is_bar { 1.4 } else { 0.6 },
                            if is_bar {
                                palette.timeline_grid_primary
                            } else {
                                palette.timeline_grid_secondary
                            },
                        );
                        painter.line_segment(
                            [
                                egui::pos2(x, timeline_rect.top()),
                                egui::pos2(x, timeline_rect.bottom()),
                            ],
                            stroke,
                        );
                        if is_bar {
                            painter.text(
                                egui::pos2(x + 6.0, timeline_rect.top() - 12.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("Bar {}", beat / 4 + 1),
                                egui::FontId::proportional(12.0),
                                palette.text_muted,
                            );
                        }
                    }

                    for (track_index, track) in self.tracks.iter().enumerate() {
                        let top = timeline_rect.top() + track_index as f32 * track_height;
                        for clip in &track.clips {
                            let clip_rect = egui::Rect::from_min_size(
                                egui::pos2(
                                    timeline_rect.left() + clip.start * beat_width,
                                    top + 6.0,
                                ),
                                egui::vec2(clip.length * beat_width - 8.0, track_height - 12.0),
                            );
                            painter.rect_filled(clip_rect, 10.0, clip.color.gamma_multiply(0.85));
                            painter.rect_stroke(
                                clip_rect,
                                10.0,
                                egui::Stroke::new(1.0, palette.toolbar_outline),
                            );
                            painter.text(
                                clip_rect.left_top() + egui::vec2(8.0, 18.0),
                                egui::Align2::LEFT_TOP,
                                &clip.name,
                                egui::FontId::proportional(14.0),
                                palette.text_primary,
                            );
                        }
                    }

                    let playhead_x = timeline_rect.left() + self.playhead * beat_width;
                    painter.line_segment(
                        [
                            egui::pos2(playhead_x, timeline_rect.top()),
                            egui::pos2(playhead_x, timeline_rect.bottom()),
                        ],
                        egui::Stroke::new(2.0, palette.accent),
                    );
                });
        });
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
}
