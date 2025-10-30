use super::widgets::*;
use crate::state::*;
use egui::{self, RichText};

pub struct StripMetrics {
    pub fader_h: f32,
    pub meter_w: f32,
}

impl Default for StripMetrics {
    fn default() -> Self {
        Self {
            fader_h: 160.0,
            meter_w: 8.0,
        }
    }
}

pub fn render(ui: &mut egui::Ui, props: crate::MixerProps) {
    let crate::MixerProps { state, callbacks } = props;
    ui.vertical(|ui| {
        header(ui, &mut *state);
        ui.separator();
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for ch in &mut state.channels {
                        channel_strip(ui, ch, &mut *callbacks);
                    }
                });
            });
    });
}

fn header(ui: &mut egui::Ui, state: &mut MixerState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mixer").heading());
        ui.add_space(8.0);
        if let Some(sel) = state.selected {
            if let Some(ch) = state.channels.iter_mut().find(|c| c.id == sel) {
                ui.label(format!("Selected: {}", ch.name));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small("Tip: Shift+Drag fader = fine; Double-click to reset");
        });
    });
}

fn channel_strip(ui: &mut egui::Ui, ch: &mut Channel, callbacks: &mut crate::MixerCallbacks) {
    let metrics = StripMetrics::default();
    egui::Frame::group(ui.style())
        .show(ui, |ui| {
            ui.set_min_width(120.0);
            // NAME
            {
                let mut name = ch.name.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut name).desired_width(110.0))
                    .lost_focus()
                {
                    ch.name = name;
                }
            }
            ui.add_space(4.0);
            // METERS + FADER row
            ui.horizontal(|ui| {
                // meters
                ui.vertical(|ui| {
                    let m = &ch.meter;
                    meter_vertical(
                        ui,
                        m.peak_l,
                        m.rms_l,
                        m.peak_hold_l,
                        egui::vec2(metrics.meter_w, metrics.fader_h),
                    );
                    meter_vertical(
                        ui,
                        m.peak_r,
                        m.rms_r,
                        m.peak_hold_r,
                        egui::vec2(metrics.meter_w, metrics.fader_h),
                    );
                });
                ui.add_space(6.0);
                // fader
                let mut db = ch.gain_db;
                if fader_db(ui, &mut db, -60.0..=12.0, metrics.fader_h) {
                    ch.gain_db = db;
                    (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                }
            });
            ui.add_space(6.0);
            // PAN + M/S
            ui.horizontal(|ui| {
                ui.label("Pan");
                let mut pan = ch.pan;
                if small_knob(ui, &mut pan, -1.0..=1.0, "") {
                    ch.pan = pan;
                    (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                }
                ui.separator();
                if ui
                    .selectable_label(ch.mute, "M")
                    .on_hover_text("Mute")
                    .clicked()
                {
                    ch.mute = !ch.mute;
                    (callbacks.set_mute)(ch.id, ch.mute);
                }
                if ui
                    .selectable_label(ch.solo, "S")
                    .on_hover_text("Solo")
                    .clicked()
                {
                    ch.solo = !ch.solo;
                    (callbacks.set_solo)(ch.id, ch.solo);
                }
            });
            ui.add_space(6.0);
            // INSERTS
            ui.collapsing("Inserts", |ui| {
                for (idx, ins) in ch.inserts.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let mut bypass = ins.bypass;
                        if ui
                            .selectable_label(bypass, "⏸")
                            .on_hover_text("Bypass")
                            .clicked()
                        {
                            bypass = !bypass;
                            ins.bypass = bypass;
                            (callbacks.set_insert_bypass)(ch.id, idx, bypass);
                        }
                        ui.label(egui::RichText::new(ins.name.as_str()).strong());
                        if ui.button("UI").on_hover_text("Open plugin UI").clicked() {
                            (callbacks.open_insert_ui)(ch.id, idx);
                        }
                        if ui.button("✕").on_hover_text("Remove").clicked() {
                            (callbacks.remove_insert)(ch.id, idx);
                        }
                    });
                }
                if ui.button("➕ Add Insert").clicked() {
                    (callbacks.open_insert_browser)(ch.id, None);
                }
            });
            // SENDS
            if !ch.is_master {
                ui.collapsing("Sends", |ui| {
                    for s in &mut ch.sends {
                        ui.horizontal(|ui| {
                            let label = format!("Send {}", ((b'A' + s.id) as char));
                            ui.label(label);
                            let mut lvl = s.level;
                            if small_knob(ui, &mut lvl, 0.0..=1.0, "") {
                                s.level = lvl;
                                (callbacks.configure_send)(ch.id, s.id, s.level);
                            }
                        });
                    }
                });
            }
            ui.add_space(4.0);
            // SELECT
            if ui.button("Select").clicked() {
                // selection is stored on MixerState outside; handled by caller if needed
            }
        })
        .response
        .context_menu(|ui| {
            if ui.button("Add Insert…").clicked() {
                (callbacks.open_insert_browser)(ch.id, None);
                ui.close_menu();
            }
        });
}
