use eframe::egui::{self, RichText};
use harmoniq_ui::{Fader, HarmoniqPalette, Knob, LevelMeter};

use crate::ui::event_bus::{AppEvent, EventBus};

struct MixerTrack {
    name: String,
    volume: f32,
    pan: f32,
    meter_left: f32,
    meter_right: f32,
}

impl MixerTrack {
    fn new(name: &str, volume: f32) -> Self {
        Self {
            name: name.to_string(),
            volume,
            pan: 0.0,
            meter_left: 0.0,
            meter_right: 0.0,
        }
    }

    fn update_meter(&mut self, time: f64) {
        let base = (time * 1.3 + self.volume as f64 * 2.4).sin() as f32 * 0.5 + 0.5;
        let pan = self.pan.clamp(-1.0, 1.0);
        let left_weight = (1.0 - pan) * 0.5;
        let right_weight = (1.0 + pan) * 0.5;
        let gain = self.volume.clamp(0.0, 1.2);
        self.meter_left = (base * gain * left_weight).clamp(0.0, 1.0);
        self.meter_right = (base * gain * right_weight).clamp(0.0, 1.0);
    }
}

pub struct MixerPane {
    tracks: Vec<MixerTrack>,
    master: MixerTrack,
}

impl Default for MixerPane {
    fn default() -> Self {
        let tracks = vec![
            MixerTrack::new("Drums", 0.85),
            MixerTrack::new("Bass", 0.78),
            MixerTrack::new("Lead", 0.72),
            MixerTrack::new("Pads", 0.65),
        ];
        let master = MixerTrack::new("Master", 0.9);
        Self { tracks, master }
    }
}

impl MixerPane {
    pub fn ui(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, event_bus: &EventBus) {
        let time = ui.ctx().input(|i| i.time);
        for track in &mut self.tracks {
            track.update_meter(time);
        }
        self.master.update_meter(time);

        ui.vertical(|ui| {
            ui.heading(RichText::new("Mixer").color(palette.text_primary));
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_source("mixer_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for track in &mut self.tracks {
                            Self::draw_channel_strip(ui, palette, track, event_bus);
                        }
                        ui.add_space(24.0);
                        Self::draw_channel_strip(ui, palette, &mut self.master, event_bus);
                    });
                });
        });
    }

    fn draw_channel_strip(
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        track: &mut MixerTrack,
        event_bus: &EventBus,
    ) {
        egui::Frame::none()
            .fill(palette.panel_alt)
            .stroke(egui::Stroke::new(1.0, palette.toolbar_outline))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(&track.name)
                                .color(palette.text_primary)
                                .strong(),
                        );
                    });
                    ui.add_space(12.0);
                    let mut volume = track.volume;
                    if ui
                        .add(Fader::new(&mut volume, 0.0, 1.2, 0.75, palette).with_height(160.0))
                        .changed()
                    {
                        track.volume = volume;
                    }
                    ui.add_space(12.0);
                    let mut pan = track.pan;
                    if ui
                        .add(
                            Knob::new(&mut pan, -1.0, 1.0, 0.0, "Pan", palette).with_diameter(48.0),
                        )
                        .changed()
                    {
                        track.pan = pan;
                    }
                    ui.add_space(12.0);
                    ui.add(LevelMeter::new(palette).with_levels(
                        track.meter_left,
                        track.meter_right,
                        (track.meter_left + track.meter_right) * 0.5,
                    ));
                    ui.add_space(8.0);
                    if ui.button("Insert FX").clicked() {
                        event_bus.publish(AppEvent::RequestRepaint);
                    }
                });
            });
        ui.add_space(12.0);
    }

    pub fn cpu_estimate(&self) -> f32 {
        let avg = self
            .tracks
            .iter()
            .map(|track| track.volume)
            .chain(std::iter::once(self.master.volume))
            .sum::<f32>()
            / (self.tracks.len() as f32 + 1.0);
        (0.25 + avg * 0.5).clamp(0.1, 0.95)
    }

    pub fn master_meter(&self) -> (f32, f32) {
        (self.master.meter_left, self.master.meter_right)
    }
}
