use std::time::{Duration, Instant};

use eframe::egui::{self, RichText, Slider};
use harmoniq_ui::{Fader, HarmoniqPalette, Knob, LevelMeter};

use crate::ui::event_bus::{AppEvent, EventBus};
use crate::ui::focus::InputFocus;
use crate::ui::workspace::WorkspacePane;

struct MixerTrack {
    name: String,
    volume: f32,
    pan: f32,
    meter_left: f32,
    meter_right: f32,
    width: f32,
    phase_invert: bool,
    aux_send: f32,
    aux_pre: bool,
    true_peak: f32,
    short_term: f32,
    phase_correlation: f32,
}

impl MixerTrack {
    fn new(name: &str, volume: f32) -> Self {
        Self {
            name: name.to_string(),
            volume,
            pan: 0.0,
            meter_left: 0.0,
            meter_right: 0.0,
            width: 1.0,
            phase_invert: false,
            aux_send: -12.0,
            aux_pre: false,
            true_peak: f32::NEG_INFINITY,
            short_term: f32::NEG_INFINITY,
            phase_correlation: 1.0,
        }
    }

    fn update_meter(&mut self, time: f64) {
        let base = (time * 1.3 + self.volume as f64 * 2.4).sin() as f32 * 0.5 + 0.5;
        let pan = self.pan.clamp(-1.0, 1.0);
        let left_weight = (1.0 - pan) * 0.5;
        let right_weight = (1.0 + pan) * 0.5;
        let gain = self.volume.clamp(0.0, 1.2);
        let width = self.width.clamp(0.0, 2.0);
        let mid = base * gain;
        let side = (left_weight - right_weight).abs() * gain * (width - 1.0).abs() * 0.25;
        let phase = if self.phase_invert { -1.0 } else { 1.0 };
        self.meter_left = (mid * left_weight + side).clamp(0.0, 1.0);
        self.meter_right = (mid * right_weight - side).clamp(0.0, 1.0);
        let peak = self.meter_left.max(self.meter_right).max(1e-4);
        self.true_peak = 20.0 * peak.log10();
        let energy = ((self.meter_left * self.meter_left) + (self.meter_right * self.meter_right))
            * 0.5
            * 0.001;
        self.short_term = if energy > 0.0 {
            -0.691 + 10.0 * energy.log10()
        } else {
            f32::NEG_INFINITY
        };
        let correlation = ((1.0 - pan.abs()) * phase).clamp(-1.0, 1.0);
        self.phase_correlation = correlation;
    }
}

pub struct MixerPane {
    tracks: Vec<MixerTrack>,
    master: MixerTrack,
    meter_cache: Vec<MeterSnapshot>,
    meter_refresh: Duration,
    last_meter_refresh: Instant,
}

#[derive(Clone, Copy, Default)]
struct MeterSnapshot {
    left: f32,
    right: f32,
    rms: f32,
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
        Self {
            tracks,
            master,
            meter_cache: Vec::new(),
            meter_refresh: Duration::from_secs_f64(1.0 / 144.0),
            last_meter_refresh: Instant::now(),
        }
    }
}

impl MixerPane {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        event_bus: &EventBus,
        focus: &mut InputFocus,
    ) {
        let ctx = ui.ctx().clone();
        let time = ctx.input(|i| i.time);
        let stable_dt = ctx.input(|i| i.stable_dt);
        if stable_dt > f32::EPSILON {
            let hz = (1.0_f32 / stable_dt).clamp(60.0_f32, 144.0_f32);
            self.meter_refresh = Duration::from_secs_f64((1.0 / hz) as f64);
        }
        let refresh_due =
            self.last_meter_refresh.elapsed() >= self.meter_refresh || self.meter_cache.is_empty();
        self.ensure_meter_cache();

        if refresh_due {
            self.last_meter_refresh = Instant::now();
        }

        for (index, track) in self.tracks.iter_mut().enumerate() {
            if refresh_due {
                track.update_meter(time);
                self.meter_cache[index] = MeterSnapshot {
                    left: track.meter_left,
                    right: track.meter_right,
                    rms: (track.meter_left + track.meter_right) * 0.5,
                };
            }
        }

        if refresh_due {
            self.master.update_meter(time);
            let master_index = self.tracks.len();
            if let Some(entry) = self.meter_cache.get_mut(master_index) {
                *entry = MeterSnapshot {
                    left: self.master.meter_left,
                    right: self.master.meter_right,
                    rms: (self.master.meter_left + self.master.meter_right) * 0.5,
                };
            }
        }

        ui.vertical(|ui| {
            ui.heading(RichText::new("Mixer").color(palette.text_primary));
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_source("mixer_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, track) in self.tracks.iter_mut().enumerate() {
                            let meter = self.meter_cache.get(index).copied().unwrap_or_default();
                            Self::draw_channel_strip(ui, palette, track, event_bus, meter);
                        }
                        ui.add_space(24.0);
                        let master_index = self.tracks.len();
                        let master_meter = self
                            .meter_cache
                            .get(master_index)
                            .copied()
                            .unwrap_or_default();
                        Self::draw_channel_strip(
                            ui,
                            palette,
                            &mut self.master,
                            event_bus,
                            master_meter,
                        );
                    });
                });
        });

        focus.track_pane_interaction(&ctx, ui.min_rect(), WorkspacePane::Mixer);
    }

    fn draw_channel_strip(
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        track: &mut MixerTrack,
        event_bus: &EventBus,
        meter: MeterSnapshot,
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
                    ui.add_space(8.0);
                    let mut width = track.width;
                    if ui
                        .add(
                            Knob::new(&mut width, 0.0, 2.0, 1.0, "Width", palette)
                                .with_diameter(44.0),
                        )
                        .changed()
                    {
                        track.width = width;
                    }
                    ui.add_space(12.0);
                    ui.add(LevelMeter::new(palette).with_levels(
                        meter.left,
                        meter.right,
                        meter.rms,
                    ));
                    ui.add_space(8.0);
                    let mut invert = track.phase_invert;
                    if ui.checkbox(&mut invert, "Phase Invert").changed() {
                        track.phase_invert = invert;
                    }
                    ui.add_space(6.0);
                    ui.add(Slider::new(&mut track.aux_send, -60.0..=6.0).text("Aux Send (dB)"));
                    ui.checkbox(&mut track.aux_pre, "Pre-Fader");
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("True Peak: {:.1} dBFS", track.true_peak))
                            .color(palette.text_muted),
                    );
                    ui.label(
                        RichText::new(format!("Short-Term: {:.1} LUFS", track.short_term))
                            .color(palette.text_muted),
                    );
                    ui.label(
                        RichText::new(format!("Phase Corr: {:.2}", track.phase_correlation))
                            .color(palette.text_muted),
                    );
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

    fn ensure_meter_cache(&mut self) {
        let required = self.tracks.len() + 1;
        if self.meter_cache.len() != required {
            self.meter_cache.resize(required, MeterSnapshot::default());
        }
    }
}
