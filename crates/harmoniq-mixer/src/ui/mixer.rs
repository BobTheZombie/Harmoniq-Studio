use crate::state::{InsertSlot, MixerState};
use egui::{self, Align, ComboBox, Frame, Margin, RichText, Rounding, Slider, Stroke, Vec2};
use harmoniq_ui::{Fader, HarmoniqPalette, LevelMeter, StateToggleButton};

pub fn render(ui: &mut egui::Ui, props: crate::MixerProps) {
    let crate::MixerProps {
        state,
        callbacks,
        palette,
    } = props;

    Frame::none()
        .fill(palette.panel.gamma_multiply(0.9))
        .inner_margin(Margin::symmetric(10.0, 8.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .rounding(Rounding::same(8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("MixConsole").heading().color(palette.accent));
                ui.add_space(10.0);

                ComboBox::from_id_source("strip_width_picker")
                    .selected_text(format!(
                        "Strip width: {}px",
                        state.layout.strip_width as i32
                    ))
                    .show_ui(ui, |ui| {
                        for (label, value) in
                            [("Narrow", 120.0f32), ("Standard", 152.0), ("Wide", 188.0)]
                        {
                            ui.selectable_value(&mut state.layout.strip_width, value, label);
                        }
                    });

                ui.separator();

                if ui
                    .button(RichText::new("Reset clips").color(palette.text_primary))
                    .clicked()
                {
                    state.reset_peaks_all();
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

    let strip_width = state.layout.strip_width.clamp(110.0, 220.0);

    egui::ScrollArea::horizontal()
        .id_source("mixer_strip_scroll_channel_strip")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            ui.horizontal(|ui| {
                let mut master = None;
                for idx in 0..state.channels.len() {
                    let is_master = state.channels[idx].is_master;
                    if is_master {
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
        palette.mixer_strip_bg.gamma_multiply(0.5)
    } else {
        palette.mixer_strip_bg.gamma_multiply(0.92)
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
                strip_header(ui, &channel, palette, is_master);

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    meter_and_fader(ui, &mut channel, palette, callbacks);
                    ui.add_space(6.0);
                    insert_column(ui, &mut channel, palette, callbacks);
                    ui.add_space(4.0);
                    send_column(ui, &channel, palette);
                });

                ui.add_space(8.0);

                control_row(ui, &mut channel, palette, callbacks);

                ui.add_space(4.0);

                pan_row(ui, &mut channel, palette, callbacks);
            });
        });

    state.channels[channel_index] = channel;
}

fn strip_header(
    ui: &mut egui::Ui,
    channel: &crate::state::Channel,
    palette: &HarmoniqPalette,
    is_master: bool,
) {
    let title_bg = if is_master {
        palette.mixer_strip_header_selected
    } else {
        palette.mixer_strip_header
    };
    Frame::none()
        .fill(title_bg)
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .inner_margin(Margin::symmetric(6.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(channel.name.clone())
                        .strong()
                        .color(if is_master {
                            palette.accent
                        } else {
                            palette.text_primary
                        })
                        .size(14.0),
                );

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if channel.meter.clip_l || channel.meter.clip_r {
                        ui.label(RichText::new("CLIP").color(palette.warning).strong());
                    }
                });
            });
        });
}

fn meter_and_fader(
    ui: &mut egui::Ui,
    channel: &mut crate::state::Channel,
    palette: &HarmoniqPalette,
    callbacks: &mut crate::MixerCallbacks,
) {
    ui.vertical(|ui| {
        let meter = LevelMeter::new(palette)
            .with_size(Vec2::new(18.0, 220.0))
            .with_levels(
                channel.meter.peak_l,
                channel.meter.peak_r,
                channel.meter.rms_l,
            )
            .with_clip(channel.meter.clip_l, channel.meter.clip_r);
        let response = ui.add(meter.interactive(true));
        if response.clicked() {
            channel.meter.clip_l = false;
            channel.meter.clip_r = false;
        }

        ui.add_space(6.0);

        let mut gain = channel.gain_db;
        if ui
            .add(Fader::new(&mut gain, -60.0, 12.0, 0.0, palette).with_height(220.0))
            .on_hover_text("Fader")
            .changed()
        {
            channel.gain_db = gain;
            (callbacks.set_gain_pan)(channel.id, gain, channel.pan);
        }
    });
}

fn insert_column(
    ui: &mut egui::Ui,
    channel: &mut crate::state::Channel,
    palette: &HarmoniqPalette,
    callbacks: &mut crate::MixerCallbacks,
) {
    Frame::none()
        .fill(palette.mixer_strip_bg.gamma_multiply(0.95))
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .inner_margin(Margin::symmetric(6.0, 4.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Inserts").color(palette.text_muted).small());

                let rows = channel.inserts.len().max(3);

                for idx in 0..rows {
                    let (name, format, bypassed, has_plugin) = match channel.inserts.get(idx) {
                        Some(slot) if slot.plugin_uid.is_some() => (
                            slot.name.clone(),
                            slot.format
                                .map(|f| f.label().to_string())
                                .unwrap_or_else(|| "".to_string()),
                            slot.bypass,
                            true,
                        ),
                        _ => ("Empty".to_string(), String::new(), false, false),
                    };

                    let response = Frame::none()
                        .fill(if bypassed {
                            palette.mixer_slot_bg.gamma_multiply(0.8)
                        } else {
                            palette.mixer_slot_bg
                        })
                        .rounding(Rounding::same(3.0))
                        .stroke(Stroke::new(1.0, palette.mixer_slot_border))
                        .inner_margin(Margin::symmetric(4.0, 3.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(name.clone()).small().color(if bypassed {
                                    palette.text_muted
                                } else {
                                    palette.text_primary
                                }));

                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if !format.is_empty() {
                                        ui.label(RichText::new(format.clone()).small());
                                    }
                                });
                            });
                        })
                        .response;

                    if response.clicked() {
                        if has_plugin {
                            (callbacks.open_insert_ui)(channel.id, idx);
                        } else {
                            (callbacks.open_insert_browser)(channel.id, Some(idx));
                            channel.ensure_insert_slot(idx);
                        }
                    }

                    response.context_menu(|ui| {
                        if has_plugin {
                            if let Some(slot) = channel.inserts.get_mut(idx) {
                                if ui.checkbox(&mut slot.bypass, "Bypass").changed() {
                                    (callbacks.set_insert_bypass)(channel.id, idx, slot.bypass);
                                }

                                if ui.button("Open editor").clicked() {
                                    (callbacks.open_insert_ui)(channel.id, idx);
                                    ui.close_menu();
                                }

                                if ui.button("Remove").clicked() {
                                    *slot = InsertSlot::empty();
                                    (callbacks.remove_insert)(channel.id, idx);
                                    ui.close_menu();
                                }
                            }
                        } else if ui.button("Add effect...").clicked() {
                            (callbacks.open_insert_browser)(channel.id, Some(idx));
                            channel.ensure_insert_slot(idx);
                            ui.close_menu();
                        }
                    });
                }

                ui.add_space(4.0);

                if ui
                    .button(RichText::new("+ Add effect").color(palette.text_primary))
                    .clicked()
                {
                    let next_slot = channel.inserts.len();
                    (callbacks.open_insert_browser)(channel.id, None);
                    channel.ensure_insert_slot(next_slot);
                }
            });
        });
}

fn send_column(ui: &mut egui::Ui, channel: &crate::state::Channel, palette: &HarmoniqPalette) {
    Frame::none()
        .fill(palette.mixer_strip_bg.gamma_multiply(0.95))
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .inner_margin(Margin::symmetric(6.0, 4.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Sends").color(palette.text_muted).small());

                let rows = channel.sends.len().max(2);

                for idx in 0..rows {
                    let slot_name = channel
                        .sends
                        .get(idx)
                        .and_then(|send| send.target.clone())
                        .unwrap_or_else(|| "Unassigned".to_string());

                    Frame::none()
                        .fill(palette.mixer_slot_bg)
                        .rounding(Rounding::same(3.0))
                        .stroke(Stroke::new(1.0, palette.mixer_slot_border))
                        .inner_margin(Margin::symmetric(4.0, 3.0))
                        .show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new(slot_name).small());
                            });
                        });
                }
            });
        });
}

fn control_row(
    ui: &mut egui::Ui,
    channel: &mut crate::state::Channel,
    palette: &HarmoniqPalette,
    callbacks: &mut crate::MixerCallbacks,
) {
    ui.horizontal(|ui| {
        let mute = ui.add(StateToggleButton::new(&mut channel.mute, "Mute", palette));
        if mute.changed() {
            (callbacks.set_mute)(channel.id, channel.mute);
        }

        let solo = ui.add(StateToggleButton::new(&mut channel.solo, "Solo", palette));
        if solo.changed() {
            (callbacks.set_solo)(channel.id, channel.solo);
        }

        ui.add(StateToggleButton::new(
            &mut channel.record_enable,
            "Record",
            palette,
        ));
    });
}

fn pan_row(
    ui: &mut egui::Ui,
    channel: &mut crate::state::Channel,
    palette: &HarmoniqPalette,
    callbacks: &mut crate::MixerCallbacks,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Pan").color(palette.text_muted));
        let mut pan = channel.pan;
        if ui
            .add(Slider::new(&mut pan, -1.0..=1.0).clamp_to_range(true))
            .changed()
        {
            channel.pan = pan;
            (callbacks.set_gain_pan)(channel.id, channel.gain_db, pan);
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Width").color(palette.text_muted));
        let mut width = channel.stereo_separation;
        if ui
            .add(Slider::new(&mut width, 0.0..=2.0).clamp_to_range(true))
            .changed()
        {
            channel.stereo_separation = width;
            (callbacks.set_stereo_separation)(channel.id, width);
        }
    });
}
