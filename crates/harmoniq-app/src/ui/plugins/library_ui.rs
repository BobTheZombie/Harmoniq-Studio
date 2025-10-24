use eframe::egui::{self, RichText};

use harmoniq_plugin_db::{PluginEntry, PluginStore};

pub struct PluginLibraryUi {
    entries: Vec<PluginEntry>,
}

impl PluginLibraryUi {
    pub fn new(store: &PluginStore) -> Self {
        Self {
            entries: store.plugins(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("Plugin Library");
        if self.entries.is_empty() {
            ui.label("No plugins installed yet.");
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &self.entries {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&entry.name).strong());
                    if let Some(vendor) = &entry.vendor {
                        ui.label(RichText::new(vendor));
                    }
                });
            }
        });
    }
}
