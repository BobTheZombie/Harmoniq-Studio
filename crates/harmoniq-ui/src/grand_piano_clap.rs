use egui::{self, Align};

use crate::{Fader, HarmoniqPalette, Knob};

/// Bundles mutable references to the grand piano clap parameters so the UI can
/// manipulate them directly.
pub struct GrandPianoClapParams<'a> {
    pub piano_level: &'a mut f32,
    pub clap_level: &'a mut f32,
    pub tone: &'a mut f32,
    pub sparkle: &'a mut f32,
    pub body: &'a mut f32,
    pub width: &'a mut f32,
    pub clap_delay: &'a mut f32,
    pub clap_tightness: &'a mut f32,
    pub attack: &'a mut f32,
    pub decay: &'a mut f32,
    pub sustain: &'a mut f32,
    pub release: &'a mut f32,
}

impl<'a> GrandPianoClapParams<'a> {
    pub fn new(
        piano_level: &'a mut f32,
        clap_level: &'a mut f32,
        tone: &'a mut f32,
        sparkle: &'a mut f32,
        body: &'a mut f32,
        width: &'a mut f32,
        clap_delay: &'a mut f32,
        clap_tightness: &'a mut f32,
        attack: &'a mut f32,
        decay: &'a mut f32,
        sustain: &'a mut f32,
        release: &'a mut f32,
    ) -> Self {
        Self {
            piano_level,
            clap_level,
            tone,
            sparkle,
            body,
            width,
            clap_delay,
            clap_tightness,
            attack,
            decay,
            sustain,
            release,
        }
    }
}

/// Renders the custom UI for the Grand Piano Clap instrument, returning the
/// [`egui::Response`] from the surrounding group.
pub fn show_grand_piano_clap_ui(
    ui: &mut egui::Ui,
    params: GrandPianoClapParams<'_>,
    palette: &HarmoniqPalette,
) -> egui::Response {
    let mut params = params;
    ui.group(|ui| {
        ui.vertical_centered(|ui| {
            ui.heading("Grand Piano Clap");
            ui.add_space(6.0);
        });
        ui.horizontal(|ui| {
            ui.add(Knob::new(
                params.piano_level,
                0.0,
                1.5,
                0.85,
                "Piano",
                palette,
            ));
            ui.add(Knob::new(
                params.clap_level,
                0.0,
                1.5,
                0.65,
                "Clap",
                palette,
            ));
            ui.add(Knob::new(params.tone, 0.0, 1.0, 0.55, "Tone", palette));
            ui.add(Knob::new(
                params.sparkle,
                0.0,
                1.0,
                0.35,
                "Sparkle",
                palette,
            ));
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(Knob::new(params.body, 0.0, 1.0, 0.35, "Body", palette));
            ui.add(Knob::new(params.width, 0.0, 1.0, 0.65, "Width", palette));
            ui.add(Knob::new(
                params.clap_delay,
                0.0,
                0.25,
                0.05,
                "Delay",
                palette,
            ));
            ui.add(Knob::new(
                params.clap_tightness,
                0.5,
                1.5,
                1.0,
                "Tight",
                palette,
            ));
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.with_layout(egui::Layout::top_down(Align::Center), |ui| {
                    ui.label("Envelope");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            Fader::new(params.attack, 0.001, 0.2, 0.01, palette).with_height(110.0),
                        );
                        ui.add(
                            Fader::new(params.decay, 0.05, 2.0, 0.35, palette).with_height(110.0),
                        );
                        ui.add(
                            Fader::new(params.sustain, 0.0, 1.0, 0.65, palette).with_height(110.0),
                        );
                        ui.add(
                            Fader::new(params.release, 0.05, 2.5, 0.45, palette).with_height(110.0),
                        );
                    });
                });
            });
        });
    })
    .response
}
