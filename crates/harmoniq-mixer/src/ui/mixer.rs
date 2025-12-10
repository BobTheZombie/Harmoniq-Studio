use crate::state::MixerState;
use egui::{
    self, Align, CollapsingHeader, Frame, Grid, Margin, RichText, Rounding, Slider, Stroke, Vec2,
};
use harmoniq_ui::{Fader, HarmoniqPalette, LevelMeter, StateToggleButton};

pub fn render(ui: &mut egui::Ui, props: crate::MixerProps) {
    let crate::MixerProps {
        state,
        callbacks,
        palette,
    } = props;

    Frame::none()
        .fill(palette.panel.gamma_multiply(0.85))
        .inner_margin(Margin::symmetric(8.0, 6.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .rounding(Rounding::same(6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mixer").heading().color(palette.accent));
                ui.add_space(10.0);

                let mut width = state.layout.strip_width.clamp(50.0, 100.0);
                if ui
                    .add(
                        Slider::new(&mut width, 50.0..=100.0)
                            .text("Strip width")
                            .step_by(1.0),
                    )
                    .changed()
                {
                    state.layout.strip_width = width;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new("+ Track").color(palette.text_primary))
                        .clicked()
                    {
                        (callbacks.add_channel)();
                    }
                });
            });
        });

    ui.add_space(6.0);

    let strip_width = state.layout.strip_width.clamp(50.0, 100.0);

    egui::ScrollArea::horizontal()
        .id_source("mixer_strip_scroll_compact")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            ui.horizontal(|ui| {
                let mut master = None;
                for (idx, ch) in state.channels.iter().enumerate() {
                    if ch.is_master {
                        master = Some(idx);
                        continue;
                    }
                    strip_ui(ui, idx, state, callbacks, palette, strip_width, false);
                }

                if let Some(idx) = master {
                    strip_ui(ui, idx, state, callbacks, palette, strip_width, true);
                }
            });
        });
}

fn strip_ui(
    ui: &mut egui::Ui,
    channel_index: usize,
    state: &mut MixerState,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    strip_width: f32,
    is_master: bool,
) {
    let mut channel = state.channels[channel_index].clone();

    let bg = if is_master {
        palette.mixer_strip_bg.gamma_multiply(0.55)
    } else {
        palette.mixer_strip_bg.gamma_multiply(0.9)
    };

    Frame::none()
        .fill(bg)
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .inner_margin(Margin::symmetric(6.0, 6.0))
        .show(ui, |ui| {
            ui.set_width(strip_width);
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);

            ui.vertical(|ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(channel.name.clone())
                            .strong()
                            .color(if is_master {
                                palette.accent
                            } else {
                                palette.text_primary
                            })
                            .size(13.0),
                    );
                });

                CollapsingHeader::new("Details")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            let mut gain = channel.gain_db;
                            if ui
                                .add(Slider::new(&mut gain, -60.0..=12.0).text("Gain (dB)"))
                                .changed()
                            {
                                channel.gain_db = gain;
                                (callbacks.set_gain_pan)(channel.id, gain, channel.pan);
                            }

                            ui.separator();
                            ui.label(RichText::new("FX / Inserts").color(palette.text_muted));
                            Grid::new(("fx_slots", channel.id))
                                .num_columns(1)
                                .show(ui, |ui| {
                                    for slot in 0..3 {
                                        ui.label(
                                            RichText::new(format!("Slot {}", slot + 1)).small(),
                                        );
                                        ui.end_row();
                                    }
                                });
                        });
                    });

                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    let time = ui.input(|i| i.time) as f32;
                    let sim = (time * 1.7 + channel.id as f32).sin().abs() * 0.4;
                    let peak_l = channel.meter.peak_l.max(sim);
                    let peak_r = channel.meter.peak_r.max(sim * 0.9);
                    let rms = channel.meter.rms_l.max(sim * 0.6);

                    let meter = LevelMeter::new(palette)
                        .with_size(Vec2::new(12.0, 180.0))
                        .with_levels(peak_l, peak_r, rms)
                        .with_clip(channel.meter.clip_l, channel.meter.clip_r);
                    ui.add(meter);

                    let mut gain = channel.gain_db;
                    if ui
                        .add(Fader::new(&mut gain, -60.0, 12.0, 0.0, palette).with_height(180.0))
                        .on_hover_text("Volume")
                        .changed()
                    {
                        channel.gain_db = gain;
                        (callbacks.set_gain_pan)(channel.id, gain, channel.pan);
                    }
                });

                let mut pan = channel.pan;
                if ui
                    .add(
                        Slider::new(&mut pan, -1.0..=1.0)
                            .text("Pan")
                            .clamp_to_range(true),
                    )
                    .changed()
                {
                    channel.pan = pan;
                    (callbacks.set_gain_pan)(channel.id, channel.gain_db, pan);
                }

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let mute = ui.add(StateToggleButton::new(&mut channel.mute, "M", palette));
                        if mute.changed() {
                            (callbacks.set_mute)(channel.id, channel.mute);
                        }

                        let solo = ui.add(StateToggleButton::new(&mut channel.solo, "S", palette));
                        if solo.changed() {
                            (callbacks.set_solo)(channel.id, channel.solo);
                        }

                        ui.add(StateToggleButton::new(
                            &mut channel.record_enable,
                            "R",
                            palette,
                        ));
                    });

                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("FX").small().color(palette.text_muted));
                        for _ in 0..2 {
                            Frame::none()
                                .fill(palette.mixer_slot_bg)
                                .rounding(Rounding::same(4.0))
                                .stroke(Stroke::new(1.0, palette.mixer_slot_border))
                                .inner_margin(Margin::symmetric(4.0, 3.0))
                                .show(ui, |ui| {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(RichText::new("Insert").small());
                                    });
                                });
                        }
                    });
                });
            });
        });

    state.channels[channel_index] = channel;
}
