use eframe::egui::{self, Color32, RichText};
use harmoniq_ui::{HarmoniqPalette, Knob, StepToggle};

use crate::ui::event_bus::{AppEvent, EventBus};

#[derive(Clone)]
struct Channel {
    name: String,
    color: Color32,
    steps: Vec<bool>,
    volume: f32,
    pan: f32,
}

impl Channel {
    fn new(name: &str, color: Color32) -> Self {
        Self {
            name: name.to_string(),
            color,
            steps: vec![false; 16],
            volume: 0.75,
            pan: 0.0,
        }
    }
}

pub struct ChannelRackPane {
    channels: Vec<Channel>,
}

impl ChannelRackPane {
    fn seed_channels() -> Vec<Channel> {
        vec![
            Channel::new("Kick", Color32::from_rgb(240, 170, 100)),
            Channel::new("Snare", Color32::from_rgb(160, 200, 240)),
            Channel::new("Hat", Color32::from_rgb(190, 200, 120)),
            Channel::new("Bass", Color32::from_rgb(150, 140, 220)),
        ]
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, event_bus: &EventBus) {
        if self.channels.is_empty() {
            self.channels = Self::seed_channels();
        }
        ui.vertical(|ui| {
            ui.heading(RichText::new("Channel Rack").color(palette.text_primary));
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
                                for step in 0..channel.steps.len() {
                                    let accent = channel.color.gamma_multiply(if step % 4 == 0 {
                                        0.9
                                    } else {
                                        0.7
                                    });
                                    let toggle = ui.add(
                                        StepToggle::new(palette, accent)
                                            .active(channel.steps[step])
                                            .emphasise(step % 4 == 0)
                                            .with_size(egui::vec2(20.0, 34.0)),
                                    );
                                    if toggle.clicked() {
                                        channel.steps[step] = !channel.steps[step];
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
                                            self.channels.insert(index + 1, clone);
                                        }
                                    },
                                );
                            });
                        });
                    });
                ui.add_space(8.0);
            }
            if ui.button("Add Channel").clicked() {
                self.channels.push(Channel::new(
                    &format!("Channel {}", self.channels.len() + 1),
                    palette.accent,
                ));
            }
        });
    }
}

impl Default for ChannelRackPane {
    fn default() -> Self {
        Self {
            channels: Self::seed_channels(),
        }
    }
}
