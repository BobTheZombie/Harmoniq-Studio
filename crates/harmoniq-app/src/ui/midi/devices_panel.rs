use eframe::egui::{self, RichText};
use harmoniq_midi::config::MidiSettings;

/// UI panel for managing MIDI devices.
#[derive(Default)]
pub struct MidiDevicesPanel {
    settings: MidiSettings,
    is_open: bool,
}

impl MidiDevicesPanel {
    /// Open the panel with the provided settings snapshot.
    pub fn open(&mut self, settings: MidiSettings) {
        self.settings = settings;
        self.is_open = true;
    }

    /// Close the panel.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Whether the panel is currently visible.
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Render the panel contents.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<MidiSettings> {
        if !self.is_open {
            return None;
        }
        let mut updated = None;
        egui::Window::new("MIDI Devices")
            .open(&mut self.is_open)
            .show(ctx, |ui| {
                ui.heading("MIDI Inputs");
                if self.settings.inputs.is_empty() {
                    ui.label(RichText::new("No MIDI inputs configured").italics());
                } else {
                    for input in &mut self.settings.inputs {
                        ui.group(|ui| {
                            ui.checkbox(&mut input.enabled, &input.name);
                            ui.horizontal(|ui| {
                                ui.label("Port index:");
                                ui.add(
                                    egui::DragValue::new(&mut input.port_index).clamp_range(0..=64),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Transpose:");
                                ui.add(egui::Slider::new(&mut input.transpose, -24..=24));
                            });
                        });
                    }
                }
                ui.separator();
                ui.checkbox(
                    &mut self.settings.qwerty_enabled,
                    "Enable QWERTY keyboard fallback",
                );
                if ui.button("Save").clicked() {
                    updated = Some(self.settings.clone());
                }
            });
        updated
    }
}
