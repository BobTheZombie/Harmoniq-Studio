use eframe::egui::{RichText, Ui};
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};

pub fn strip_context_menu(ui: &mut Ui, api: &dyn MixerUiApi, index: usize, info: &UiStripInfo) {
    ui.label(RichText::new(&info.name).strong());
    ui.separator();
    if ui.button("Rename…").clicked() {
        ui.close_menu();
    }
    if ui.button("Choose Color…").clicked() {
        ui.close_menu();
    }
    ui.separator();
    if ui.button("Reset Fader").clicked() {
        api.set_fader_db(index, 0.0);
        ui.close_menu();
    }
    if ui.button("Reset Pan").clicked() {
        api.set_pan(index, 0.0);
        ui.close_menu();
    }
    if ui.button("Reset Width").clicked() {
        api.set_width(index, 1.0);
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
    if ui.button("Route to…").clicked() {
        ui.close_menu();
    }
    if ui.button("Save Strip Preset…").clicked() {
        ui.close_menu();
    }
    if ui.button("Load Strip Preset…").clicked() {
        ui.close_menu();
    }
}
