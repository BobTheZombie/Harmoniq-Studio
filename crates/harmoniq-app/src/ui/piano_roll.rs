use eframe::egui::{self, Color32, RichText};
use harmoniq_ui::{HarmoniqPalette, NoteBlock};

use crate::ui::event_bus::EventBus;
use crate::ui::focus::InputFocus;
use crate::ui::workspace::WorkspacePane;

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
    scale_enabled: bool,
    scale_root: i32,
    scale_kind: ScaleKind,
    chord_library: Vec<ChordDefinition>,
    insert_position: f32,
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
            scale_enabled: true,
            scale_root: 0,
            scale_kind: ScaleKind::Ionian,
            chord_library: default_chord_library(),
            insert_position: 0.0,
        }
    }
}

impl PianoRollPane {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        _event_bus: &EventBus,
        focus: &mut InputFocus,
    ) {
        let beat_width = 64.0;
        let key_height = 22.0;
        let beats = 16.0;
        let total_height = (self.max_pitch - self.min_pitch + 1) as f32 * key_height;
        let total_width = beats * beat_width;

        let ctx = ui.ctx().clone();
        let mut root_rect = ui.min_rect();
        ui.vertical(|ui| {
            ui.heading(RichText::new("Piano Roll").color(palette.text_primary));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.scale_enabled, "Scale guide");
                egui::ComboBox::from_id_source("scale_root")
                    .selected_text(note_name(self.scale_root))
                    .show_ui(ui, |ui| {
                        for root in 0..12 {
                            ui.selectable_value(&mut self.scale_root, root, note_name(root));
                        }
                    });
                egui::ComboBox::from_id_source("scale_kind")
                    .selected_text(self.scale_kind.label())
                    .show_ui(ui, |ui| {
                        for kind in ScaleKind::ALL {
                            ui.selectable_value(&mut self.scale_kind, kind, kind.label());
                        }
                    });
                ui.label(RichText::new("Insert beat").color(palette.text_muted));
                ui.add(
                    egui::DragValue::new(&mut self.insert_position)
                        .clamp_range(0.0..=beats)
                        .speed(0.25),
                );
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Chord palette").color(palette.text_muted));
                for chord in &self.chord_library {
                    if ui.button(chord.name()).clicked() {
                        self.apply_chord(chord);
                    }
                }
            });
            ui.add_space(8.0);

            let scroll = egui::ScrollArea::both()
                .id_source("piano_roll_scroll")
                .show(ui, |ui| {
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(total_width + 120.0, total_height + 40.0),
                        egui::Sense::click_and_drag(),
                    );
                    let rect = response.rect.shrink2(egui::vec2(40.0, 20.0));

                    painter.rect_filled(rect, 8.0, palette.panel_alt);
                    painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, palette.toolbar_outline));

                    for pitch in self.min_pitch..=self.max_pitch {
                        let row = (self.max_pitch - pitch) as f32;
                        let y = rect.top() + row * key_height;
                        let is_white = is_white_key(pitch);
                        let mut fill = if is_white {
                            palette.panel
                        } else {
                            palette.panel_alt.gamma_multiply(0.9)
                        };
                        if self.scale_enabled
                            && !note_in_scale(pitch, self.scale_root, self.scale_kind)
                        {
                            fill = fill.gamma_multiply(0.8);
                        }
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
                                if self.selected == Some(index) {
                                    palette.clip_border_active
                                } else {
                                    palette.timeline_grid_primary
                                },
                            )
                            .selected(self.selected == Some(index)),
                        );
                        if response.clicked() {
                            self.selected = Some(index);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Quantize to scale").clicked() {
                                if self.scale_enabled {
                                    self.quantize_note_to_scale(index);
                                }
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                self.notes.remove(index);
                                self.selected = None;
                                ui.close_menu();
                            }
                        });
                    }
                });
            root_rect = root_rect.union(scroll.inner_rect);
        });

        focus.track_pane_interaction(&ctx, root_rect, WorkspacePane::PianoRoll);
    }

    fn quantize_note_to_scale(&mut self, index: usize) {
        if index >= self.notes.len() {
            return;
        }
        let note = &mut self.notes[index];
        if note_in_scale(note.pitch, self.scale_root, self.scale_kind) {
            return;
        }
        let mut pitch = note.pitch;
        for offset in 1..12 {
            if note_in_scale(pitch + offset, self.scale_root, self.scale_kind) {
                pitch += offset;
                break;
            }
            if note_in_scale(pitch - offset, self.scale_root, self.scale_kind) {
                pitch -= offset;
                break;
            }
        }
        pitch = pitch.clamp(self.min_pitch, self.max_pitch);
        note.pitch = pitch;
    }

    fn apply_chord(&mut self, chord: &ChordDefinition) {
        let base = self.scale_root + 60;
        let start = self.insert_position.max(0.0);
        let length = 1.0;
        for interval in chord.intervals() {
            let pitch = base + interval;
            if pitch < self.min_pitch || pitch > self.max_pitch {
                continue;
            }
            self.notes.push(Note {
                start,
                length,
                pitch,
                velocity: 0.85,
            });
        }
        self.notes.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.pitch.cmp(&b.pitch))
        });
    }
}

fn is_white_key(pitch: i32) -> bool {
    matches!(pitch % 12, 0 | 2 | 4 | 5 | 7 | 9 | 11)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScaleKind {
    Ionian,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian,
    Locrian,
}

impl ScaleKind {
    const ALL: [Self; 7] = [
        ScaleKind::Ionian,
        ScaleKind::Dorian,
        ScaleKind::Phrygian,
        ScaleKind::Lydian,
        ScaleKind::Mixolydian,
        ScaleKind::Aeolian,
        ScaleKind::Locrian,
    ];

    fn intervals(self) -> &'static [i32] {
        match self {
            ScaleKind::Ionian => &[0, 2, 4, 5, 7, 9, 11],
            ScaleKind::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            ScaleKind::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            ScaleKind::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            ScaleKind::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            ScaleKind::Aeolian => &[0, 2, 3, 5, 7, 8, 10],
            ScaleKind::Locrian => &[0, 1, 3, 5, 6, 8, 10],
        }
    }

    fn label(self) -> &'static str {
        match self {
            ScaleKind::Ionian => "Ionian (Major)",
            ScaleKind::Dorian => "Dorian",
            ScaleKind::Phrygian => "Phrygian",
            ScaleKind::Lydian => "Lydian",
            ScaleKind::Mixolydian => "Mixolydian",
            ScaleKind::Aeolian => "Aeolian (Minor)",
            ScaleKind::Locrian => "Locrian",
        }
    }
}

struct ChordDefinition {
    name: String,
    intervals: Vec<i32>,
}

impl ChordDefinition {
    fn name(&self) -> &str {
        &self.name
    }

    fn intervals(&self) -> impl Iterator<Item = i32> + '_ {
        self.intervals.iter().copied()
    }
}

fn default_chord_library() -> Vec<ChordDefinition> {
    vec![
        ChordDefinition {
            name: "Triad".into(),
            intervals: vec![0, 4, 7],
        },
        ChordDefinition {
            name: "Sus2".into(),
            intervals: vec![0, 2, 7],
        },
        ChordDefinition {
            name: "Sus4".into(),
            intervals: vec![0, 5, 7],
        },
        ChordDefinition {
            name: "Seventh".into(),
            intervals: vec![0, 4, 7, 10],
        },
        ChordDefinition {
            name: "Ninth".into(),
            intervals: vec![0, 2, 4, 7, 10],
        },
    ]
}

fn note_in_scale(pitch: i32, root: i32, kind: ScaleKind) -> bool {
    let offset = (pitch - root).rem_euclid(12);
    kind.intervals().contains(&offset)
}

fn note_name(root: i32) -> &'static str {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
    ];
    NAMES[(root.rem_euclid(12)) as usize]
}
