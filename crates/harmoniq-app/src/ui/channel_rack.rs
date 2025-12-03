use eframe::egui::{self, Color32, RichText};
use harmoniq_ui::{HarmoniqPalette, Knob, StepToggle};
use tracing::debug;

use crate::ui::event_bus::{AppEvent, EventBus};

const DEFAULT_STEP_COUNT: usize = 16;

#[derive(Clone)]
struct Channel {
    name: String,
    color: Color32,
    volume: f32,
    pan: f32,
}

impl Channel {
    fn new(name: &str, color: Color32) -> Self {
        Self {
            name: name.to_string(),
            color,
            volume: 0.75,
            pan: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct ChannelRackPattern {
    steps: Vec<Vec<bool>>,     // channel index -> steps
    channel_volumes: Vec<f32>, // channel index -> volume
}

impl ChannelRackPattern {
    fn new(channel_volumes: &[f32]) -> Self {
        let channel_count = channel_volumes.len();
        Self {
            steps: vec![vec![false; DEFAULT_STEP_COUNT]; channel_count],
            channel_volumes: channel_volumes.to_vec(),
        }
    }
}

#[derive(Clone)]
struct DrumKit {
    name: String,
    style: String,
    description: String,
    sounds: Vec<String>,
}

impl DrumKit {
    fn summary(&self) -> String {
        let sounds = self.sounds.join(", ");
        format!("Style: {} • Sounds: {}", self.style, sounds)
    }
}

pub struct ChannelRackPane {
    channels: Vec<Channel>,
    stock_kits: Vec<DrumKit>,
    selected_stock_kit: Option<usize>,
    patterns: Vec<ChannelRackPattern>,
    active_pattern_index: usize,
}

impl ChannelRackPane {
    fn channel_volumes(&self) -> Vec<f32> {
        self.channels.iter().map(|channel| channel.volume).collect()
    }

    fn seed_channels() -> Vec<Channel> {
        vec![
            Channel::new("Kick", Color32::from_rgb(240, 170, 100)),
            Channel::new("Snare", Color32::from_rgb(160, 200, 240)),
            Channel::new("Hat", Color32::from_rgb(190, 200, 120)),
            Channel::new("Bass", Color32::from_rgb(150, 140, 220)),
        ]
    }

    fn current_pattern_mut(&mut self) -> &mut ChannelRackPattern {
        &mut self.patterns[self.active_pattern_index]
    }

    fn current_pattern(&self) -> Option<&ChannelRackPattern> {
        self.patterns.get(self.active_pattern_index)
    }

    pub fn active_pattern_index(&self) -> usize {
        self.active_pattern_index
    }

    pub fn pattern_mut(&mut self, index: usize) -> Option<&mut ChannelRackPattern> {
        self.patterns.get_mut(index)
    }

    fn draw_pattern_selector(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette) {
        egui::Frame::none()
            .fill(palette.panel_alt)
            .rounding(egui::Rounding::same(8.0))
            .stroke(egui::Stroke::new(1.0, palette.toolbar_outline))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Patterns")
                            .color(palette.text_primary)
                            .strong(),
                    );
                    if ui.button("◀").clicked() {
                        if self.active_pattern_index == 0 {
                            self.active_pattern_index = self.patterns.len() - 1;
                        } else {
                            self.active_pattern_index -= 1;
                        }
                    }
                    ui.label(
                        RichText::new(format!(
                            "Pattern {} / {}",
                            self.active_pattern_index + 1,
                            self.patterns.len()
                        ))
                        .color(palette.text_primary),
                    );
                    if ui.button("▶").clicked() {
                        self.active_pattern_index =
                            (self.active_pattern_index + 1) % self.patterns.len();
                    }

                    if ui.button("+ Add Pattern").clicked() {
                        let pattern = ChannelRackPattern::new(&self.channel_volumes());
                        self.patterns.push(pattern);
                        self.active_pattern_index = self.patterns.len() - 1;
                    }
                });
            });
    }

    fn draw_stock_kits(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette) {
        egui::CollapsingHeader::new(RichText::new("Stock Drum Kits").color(palette.text_primary))
            .id_source("channel_rack_stock_kits")
            .default_open(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for (index, kit) in self.stock_kits.iter().enumerate() {
                    let selected = self.selected_stock_kit == Some(index);
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
                            let response = ui.selectable_label(
                                selected,
                                RichText::new(&kit.name)
                                    .color(palette.text_primary)
                                    .strong()
                                    .size(16.0),
                            );
                            if response.clicked() {
                                if selected {
                                    self.selected_stock_kit = None;
                                } else {
                                    self.selected_stock_kit = Some(index);
                                }
                            }
                            ui.label(RichText::new(kit.summary()).color(palette.text_muted));
                            ui.add_space(4.0);
                            ui.label(RichText::new(&kit.description).color(palette.text_muted));
                        });
                }
            });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, event_bus: &EventBus) {
        if self.channels.is_empty() {
            self.channels = Self::seed_channels();
            self.patterns = vec![ChannelRackPattern::new(&self.channel_volumes())];
            self.active_pattern_index = 0;
        }
        if self.patterns.is_empty() {
            self.patterns = vec![ChannelRackPattern::new(&self.channel_volumes())];
            self.active_pattern_index = 0;
        }
        let mut clone_requests: Vec<(usize, Channel, Vec<Vec<bool>>)> = Vec::new();

        ui.vertical(|ui| {
            ui.heading(RichText::new("Channel Rack").color(palette.text_primary));
            ui.add_space(6.0);
            self.draw_pattern_selector(ui, palette);
            ui.add_space(6.0);
            self.draw_stock_kits(ui, palette);
            ui.add_space(6.0);
            for (index, channel) in self.channels.iter_mut().enumerate() {
                egui::Frame::none()
                    .fill(palette.panel_alt)
                    .rounding(egui::Rounding::same(10.0))
                    .stroke(egui::Stroke::new(1.0, palette.toolbar_outline))
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&channel.name)
                                        .color(channel.color)
                                        .strong()
                                        .size(16.0),
                                );
                                if ui
                                    .small_button("🎹")
                                    .on_hover_text("Open in piano roll")
                                    .clicked()
                                {
                                    event_bus.publish(AppEvent::OpenChannelPianoRoll {
                                        channel_index: index,
                                    });
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "Vol {:.0}%",
                                                channel.volume * 100.0
                                            ))
                                            .color(palette.text_muted),
                                        );
                                    },
                                );
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let pattern = self.current_pattern_mut();
                                for step in 0..pattern.steps[index].len() {
                                    let accent = channel.color.gamma_multiply(if step % 4 == 0 {
                                        0.9
                                    } else {
                                        0.7
                                    });
                                    let toggle = ui.add(
                                        StepToggle::new(palette, accent)
                                            .active(pattern.steps[index][step])
                                            .emphasise(step % 4 == 0)
                                            .with_size(egui::vec2(20.0, 34.0)),
                                    );
                                    if toggle.clicked() {
                                        pattern.steps[index][step] = !pattern.steps[index][step];
                                        event_bus.publish(AppEvent::RequestRepaint);
                                    }
                                }
                            });
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 14.0;
                                ui.label(RichText::new("Volume").color(palette.text_muted));
                                let mut volume = channel.volume;
                                if ui
                                    .add(
                                        Knob::new(&mut volume, 0.0, 1.0, 0.75, "", palette)
                                            .with_diameter(40.0),
                                    )
                                    .changed()
                                {
                                    channel.volume = volume;
                                    for pattern in &mut self.patterns {
                                        if let Some(target) = pattern.channel_volumes.get_mut(index)
                                        {
                                            *target = volume;
                                        }
                                    }
                                }
                                ui.label(RichText::new("Pan").color(palette.text_muted));
                                let mut pan = channel.pan;
                                if ui
                                    .add(
                                        Knob::new(&mut pan, -1.0, 1.0, 0.0, "", palette)
                                            .with_diameter(40.0),
                                    )
                                    .changed()
                                {
                                    channel.pan = pan;
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Clone").clicked() {
                                            let mut clone = channel.clone();
                                            clone.name = format!("{} Copy", channel.name);
                                            let step_templates: Vec<Vec<bool>> = self
                                                .patterns
                                                .iter()
                                                .map(|p| p.steps[index].clone())
                                                .collect();
                                            clone_requests.push((index + 1, clone, step_templates));
                                        }
                                    },
                                );
                            });
                        });
                    });
                ui.add_space(8.0);
            }
            if ui.button("Add Channel").clicked() {
                let channel = Channel::new(
                    &format!("Channel {}", self.channels.len() + 1),
                    palette.accent,
                );
                for pattern in &mut self.patterns {
                    pattern.steps.push(vec![false; DEFAULT_STEP_COUNT]);
                    pattern.channel_volumes.push(channel.volume);
                }
                self.channels.push(channel);
            }
        });

        if !clone_requests.is_empty() {
            let mut offset = 0;
            for (index, channel, templates) in clone_requests {
                let insert_at = index + offset;
                self.channels.insert(insert_at, channel);
                for (pattern, template) in self.patterns.iter_mut().zip(templates) {
                    pattern.steps.insert(insert_at, template);
                    pattern
                        .channel_volumes
                        .insert(insert_at, self.channels[insert_at].volume);
                }
                offset += 1;
            }
        }
    }
}

fn stock_drum_kits() -> Vec<DrumKit> {
    vec![
        DrumKit {
            name: "Sunset Boom".into(),
            style: "Lo-Fi Chill".into(),
            description:
                "Warm cassette-saturated hits with soft transients and vinyl noise layers.".into(),
            sounds: vec![
                "Deep Kick".into(),
                "Dusty Snare".into(),
                "Lazy Hat".into(),
                "Perc Shaker".into(),
            ],
        },
        DrumKit {
            name: "Neon Pulse".into(),
            style: "Synthwave".into(),
            description: "Punchy analog drums with gated reverb perfect for retro outrun anthems."
                .into(),
            sounds: vec![
                "Thump Kick".into(),
                "Gated Snare".into(),
                "Bright Clap".into(),
                "Analog Tom".into(),
            ],
        },
        DrumKit {
            name: "Festival Sparks".into(),
            style: "EDM Mainstage".into(),
            description: "Cutting-edge drums with tuned risers and crowd-ready impacts.".into(),
            sounds: vec![
                "Sub Kick".into(),
                "Snare Stack".into(),
                "Clap Layer".into(),
                "Ride Wash".into(),
            ],
        },
        DrumKit {
            name: "City Streets".into(),
            style: "Boom Bap".into(),
            description: "Booming kicks and crispy snares sampled from dusty records.".into(),
            sounds: vec![
                "Boom Kick".into(),
                "Crack Snare".into(),
                "Closed Hat".into(),
                "Open Hat".into(),
            ],
        },
        DrumKit {
            name: "Aurora Drops".into(),
            style: "Future Bass".into(),
            description: "Lush percussion with foley textures for emotive future bass drops."
                .into(),
            sounds: vec![
                "Sub Impact".into(),
                "Glass Snare".into(),
                "Feather Hat".into(),
                "Foley Hit".into(),
            ],
        },
    ]
}

impl Default for ChannelRackPane {
    fn default() -> Self {
        let channels = Self::seed_channels();
        let volumes: Vec<f32> = channels.iter().map(|channel| channel.volume).collect();
        let patterns = vec![ChannelRackPattern::new(&volumes)];
        Self {
            channels,
            stock_kits: stock_drum_kits(),
            selected_stock_kit: None,
            patterns,
            active_pattern_index: 0,
        }
    }
}

/// Schedule step events for the current 16th-note tick.
///
/// The audio engine is expected to call this every tick of the step sequencer.
/// A standard 960 PPQ grid is used, so each 16th note advances by 240 ticks.
pub fn schedule_step_events(pattern: &ChannelRackPattern, beat: usize) {
    if pattern.steps.is_empty() {
        return;
    }

    let steps_per_bar = pattern.steps[0].len();
    if steps_per_bar == 0 {
        return;
    }

    let step_index = beat % steps_per_bar;
    let ticks_per_beat = 960u32;
    let ticks_per_step = ticks_per_beat / 4;
    let event_tick = (step_index as u32) * ticks_per_step;

    let channel_count = pattern.steps.len().min(pattern.channel_volumes.len());
    for channel_index in 0..channel_count {
        if *pattern.steps[channel_index]
            .get(step_index)
            .unwrap_or(&false)
        {
            let volume = pattern.channel_volumes[channel_index];
            trigger_channel_sound(channel_index, event_tick, volume);
        }
    }
}

fn trigger_channel_sound(channel_index: usize, tick: u32, volume: f32) {
    debug!(
        "Triggering step event for channel {channel_index} at tick {tick} with volume {:.3}",
        volume
    );
}
