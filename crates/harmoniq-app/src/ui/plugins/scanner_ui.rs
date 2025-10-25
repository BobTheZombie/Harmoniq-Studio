use eframe::egui::{self, RichText};

use harmoniq_plugin_db::PluginFormat;
use harmoniq_plugin_scanner::{ScanOptions, Scanner};

pub struct ScannerUi {
    options: ScanOptions,
    results: Vec<String>,
}

impl Default for ScannerUi {
    fn default() -> Self {
        Self {
            options: ScanOptions::default(),
            results: Vec::new(),
        }
    }
}

impl ScannerUi {
    pub fn run_ui(&mut self, ui: &mut egui::Ui, scanner: &Scanner) {
        ui.heading("Plugin Scanner");
        ui.label("Select which plugin formats to scan for.");
        let mut clap_enabled = self.options.formats.contains(&PluginFormat::Clap);
        if ui.checkbox(&mut clap_enabled, "CLAP").changed() {
            toggle_format(&mut self.options.formats, PluginFormat::Clap, clap_enabled);
        }
        let mut vst3_enabled = self.options.formats.contains(&PluginFormat::Vst3);
        if ui.checkbox(&mut vst3_enabled, "VST3").changed() {
            toggle_format(&mut self.options.formats, PluginFormat::Vst3, vst3_enabled);
        }
        let mut ovst3_enabled = self.options.formats.contains(&PluginFormat::Ovst3);
        if ui.checkbox(&mut ovst3_enabled, "OpenVST3").changed() {
            toggle_format(
                &mut self.options.formats,
                PluginFormat::Ovst3,
                ovst3_enabled,
            );
        }
        let mut harmoniq_enabled = self.options.formats.contains(&PluginFormat::Harmoniq);
        if ui.checkbox(&mut harmoniq_enabled, "Harmoniq").changed() {
            toggle_format(
                &mut self.options.formats,
                PluginFormat::Harmoniq,
                harmoniq_enabled,
            );
        }
        if ui.button("Run Scan").clicked() {
            if let Ok(entries) = scanner.scan(&self.options) {
                self.results = entries.iter().map(|entry| entry.name.clone()).collect();
            }
        }
        ui.separator();
        for entry in &self.results {
            ui.label(RichText::new(entry).strong());
        }
    }
}

fn toggle_format(formats: &mut Vec<PluginFormat>, format: PluginFormat, enabled: bool) {
    if enabled {
        if !formats.contains(&format) {
            formats.push(format);
        }
    } else if let Some(pos) = formats.iter().position(|f| *f == format) {
        formats.remove(pos);
    }
}
