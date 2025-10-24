use eframe::egui::{self, RichText, Ui};

pub fn strip_context_menu(ui: &mut Ui) {
    ui.label(RichText::new("Mixer Strip").strong());
    ui.separator();
    if ui.button("Rename...").clicked() {
        ui.close_menu();
    }
    if ui.button("Choose Color...").clicked() {
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Reset Fader").clicked() {
        ui.close_menu();
    }
    if ui.button("Reset Pan").clicked() {
        ui.close_menu();
    }
    if ui.button("Reset Width").clicked() {
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Duplicate Track").clicked() {
        ui.close_menu();
    }
    if ui.button("Freeze/Commit").clicked() {
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Route to...").clicked() {
        ui.close_menu();
    }
    if ui.button("Save Preset...").clicked() {
        ui.close_menu();
    }
    if ui.button("Load Preset...").clicked() {
        ui.close_menu();
    }
}

pub fn slot_context_menu(ui: &mut Ui) {
    if ui.button("Bypass").clicked() {
        ui.close_menu();
    }
    if ui.button("Toggle Pre/Post").clicked() {
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Replace...").clicked() {
        ui.close_menu();
    }
    if ui.button("Remove").clicked() {
        ui.close_menu();
    }
    if ui.button("Move Up").clicked() {
        ui.close_menu();
    }
    if ui.button("Move Down").clicked() {
        ui.close_menu();
    }
}
