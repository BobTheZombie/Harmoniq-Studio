use nih_plug::prelude::*;
use nih_plug_egui::{egui, widgets};
use std::sync::Arc;

use crate::Sub808Params;

pub fn editor(params: Arc<Sub808Params>) -> Option<Box<dyn Editor>> {
    let params = params.clone();

    nih_plug_egui::create_egui_editor(
        nih_plug_egui::EguiState::from_size(380, 280),
        (),
        |_ctx, _state| {},
        move |egui_ctx, setter, _state| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Harmoniq Sub808");
                });
                ui.separator();

                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    ui.label("Amp");
                    ui.add(widgets::ParamSlider::for_param(&params.level, setter));
                    ui.add(widgets::ParamSlider::for_param(&params.decay_s, setter));
                    ui.add(widgets::ParamSlider::for_param(&params.vel_sens, setter));

                    ui.separator();
                    ui.label("Pitch Thump");
                    ui.add(widgets::ParamSlider::for_param(&params.thump_amt_st, setter));
                    ui.add(widgets::ParamSlider::for_param(&params.thump_decay_s, setter));

                    let ui = &mut cols[1];
                    ui.label("Glide & Tone");
                    ui.add(widgets::ParamSlider::for_param(&params.glide_ms, setter));
                    ui.add(widgets::ParamSlider::for_param(&params.drive, setter));
                    ui.add(widgets::ParamSlider::for_param(&params.tone_hz, setter));

                    ui.separator();
                    let mut mono = params.mono.value();
                    if ui.checkbox(&mut mono, "Mono").changed() {
                        setter.begin_set_parameter(&params.mono);
                        setter.set_parameter(&params.mono, mono);
                        setter.end_set_parameter(&params.mono);
                    }
                    ui.add(widgets::ParamSlider::for_param(&params.voices, setter));
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Tip: Use low notes (C1–C2) for classic 808 subs. Increase Thump for more kick.");
                });
            });
        },
    )
}
