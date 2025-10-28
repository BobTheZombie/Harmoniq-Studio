use eframe::egui;
use harmoniq_midi::learn::{MidiLearnMap, MidiLearnMapEntry};

/// Panel displaying MIDI learn bindings.
#[derive(Default)]
pub struct MidiLearnPanel {
    is_open: bool,
    map: MidiLearnMap,
    last_message: Option<[u8; 3]>,
}

impl MidiLearnPanel {
    /// Toggle the panel.
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    /// Update the last received message for display.
    pub fn set_last_message(&mut self, msg: [u8; 3]) {
        self.last_message = Some(msg);
    }

    /// Render the panel.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.is_open {
            return;
        }
        egui::Window::new("MIDI Learn")
            .open(&mut self.is_open)
            .show(ctx, |ui| {
                if let Some(msg) = self.last_message {
                    ui.label(format!(
                        "Last message: {:02X} {:02X} {:02X}",
                        msg[0], msg[1], msg[2]
                    ));
                } else {
                    ui.label("Waiting for MIDI input…");
                }
                ui.separator();
                if self.map.entries.is_empty() {
                    ui.label("No learn bindings yet.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for entry in &self.map.entries {
                            ui.label(format!(
                                "{:02X} {:02X} {:02X} → node {} param {}",
                                entry.msg[0],
                                entry.msg[1],
                                entry.msg[2],
                                entry.target_param.0,
                                entry.target_param.1
                            ));
                        }
                    });
                }
            });
    }

    /// Access the mapping for persistence.
    pub fn map(&self) -> &MidiLearnMap {
        &self.map
    }

    /// Replace the mapping.
    pub fn set_map(&mut self, map: MidiLearnMap) {
        self.map = map;
    }

    /// Register a new binding.
    pub fn add_binding(&mut self, entry: MidiLearnMapEntry) {
        self.map.upsert(entry);
    }
}
