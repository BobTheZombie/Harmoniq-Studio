use nih_plug::prelude::*;
use nih_plug_egui::egui;
use std::sync::Arc;

use crate::Sub808Params;

pub fn editor(params: Arc<Sub808Params>) -> Box<dyn Editor> {
    nih_plug_egui::create_egui_editor(
        nih_plug_egui::EguiState::from_size(380, 280),
        move |egui_ctx, _setter, _state| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Harmoniq Sub808");
                });
                ui.separator();

                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    ui.label("Amp");
                    ui.add(params.level.to_normalized_param_slider());
                    ui.add(params.decay_s.to_normalized_param_slider());
                    ui.add(params.vel_sens.to_normalized_param_slider());

                    ui.separator();
                    ui.label("Pitch Thump");
                    ui.add(params.thump_amt_st.to_normalized_param_slider());
                    ui.add(params.thump_decay_s.to_normalized_param_slider());

                    let ui = &mut cols[1];
                    ui.label("Glide & Tone");
                    ui.add(params.glide_ms.to_normalized_param_slider());
                    ui.add(params.drive.to_normalized_param_slider());
                    ui.add(params.tone_hz.to_normalized_param_slider());

                    ui.separator();
                    ui.checkbox(params.mono.value_mut(), "Mono");
                    ui.add(params.voices.to_normalized_param_slider());
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Tip: Use low notes (C1–C2) for classic 808 subs. Increase Thump for more kick.");
                });
            });
        },
    )
}
