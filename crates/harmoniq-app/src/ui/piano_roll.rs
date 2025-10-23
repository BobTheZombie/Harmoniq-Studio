use eframe::egui::{self, Color32, RichText};
use harmoniq_ui::{HarmoniqPalette, NoteBlock};

use crate::ui::event_bus::EventBus;

struct Note {
    start: f32,
    length: f32,
    pitch: i32,
    velocity: f32,
}

impl Note {
    fn color(&self) -> Color32 {
        if self.velocity > 0.8 {
            Color32::from_rgb(240, 150, 110)
        } else {
            Color32::from_rgb(130, 180, 220)
        }
    }
}

pub struct PianoRollPane {
    notes: Vec<Note>,
    selected: Option<usize>,
    min_pitch: i32,
    max_pitch: i32,
}

impl Default for PianoRollPane {
    fn default() -> Self {
        Self {
            notes: vec![
                Note {
                    start: 0.0,
                    length: 1.0,
                    pitch: 60,
                    velocity: 0.9,
                },
                Note {
                    start: 1.5,
                    length: 0.5,
                    pitch: 64,
                    velocity: 0.6,
                },
                Note {
                    start: 2.0,
                    length: 1.0,
                    pitch: 67,
                    velocity: 0.7,
                },
                Note {
                    start: 3.0,
                    length: 0.75,
                    pitch: 72,
                    velocity: 0.8,
                },
            ],
            selected: None,
            min_pitch: 48,
            max_pitch: 84,
        }
    }
}

impl PianoRollPane {
    pub fn ui(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, _event_bus: &EventBus) {
        let beat_width = 64.0;
        let key_height = 22.0;
        let beats = 16.0;
        let total_height = (self.max_pitch - self.min_pitch + 1) as f32 * key_height;
        let total_width = beats * beat_width;

        ui.vertical(|ui| {
            ui.heading(RichText::new("Piano Roll").color(palette.text_primary));
            ui.add_space(6.0);

            egui::ScrollArea::both()
                .id_source("piano_roll_scroll")
                .show(ui, |ui| {
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(total_width + 120.0, total_height + 40.0),
                        egui::Sense::click_and_drag(),
                    );
                    let rect = response.rect.shrink2(egui::vec2(40.0, 20.0));

                    painter.rect_filled(rect, 8.0, palette.panel_alt);
                    painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, palette.toolbar_outline));

                    // Draw horizontal key lines
                    for pitch in self.min_pitch..=self.max_pitch {
                        let row = (self.max_pitch - pitch) as f32;
                        let y = rect.top() + row * key_height;
                        let is_white = is_white_key(pitch);
                        let fill = if is_white {
                            palette.panel
                        } else {
                            palette.panel_alt.gamma_multiply(0.9)
                        };
                        let row_rect = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), y),
                            egui::pos2(rect.right(), y + key_height),
                        );
                        painter.rect_filled(row_rect, 0.0, fill);
                        painter.line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(1.0, palette.timeline_grid_secondary),
                        );
                    }

                    // Draw beat grid
                    for beat in 0..=beats as i32 {
                        let x = rect.left() + beat as f32 * beat_width;
                        let is_measure = beat % 4 == 0;
                        let color = if is_measure {
                            palette.timeline_grid_primary
                        } else {
                            palette.timeline_grid_secondary
                        };
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            egui::Stroke::new(if is_measure { 1.4 } else { 0.8 }, color),
                        );
                    }

                    // Draw notes
                    for (index, note) in self.notes.iter().enumerate() {
                        if note.pitch < self.min_pitch || note.pitch > self.max_pitch {
                            continue;
                        }
                        let x = rect.left() + note.start * beat_width;
                        let width = note.length * beat_width;
                        let row = (self.max_pitch - note.pitch) as f32;
                        let y = rect.top() + row * key_height;
                        let note_rect = egui::Rect::from_min_size(
                            egui::pos2(x, y + 2.0),
                            egui::vec2(width.max(12.0), key_height - 4.0),
                        );
                        let response = ui.put(
                            note_rect,
                            NoteBlock::new(
                                note_rect,
                                palette,
                                note.color(),
                                palette.timeline_grid_primary,
                            )
                            .selected(self.selected == Some(index)),
                        );
                        if response.clicked() {
                            self.selected = Some(index);
                        }
                    }
                });
        });
    }
}

fn is_white_key(pitch: i32) -> bool {
    matches!(pitch % 12, 0 | 2 | 4 | 5 | 7 | 9 | 11)
}
