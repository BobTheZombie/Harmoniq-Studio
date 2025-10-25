use eframe::egui::{self, RichText};

use harmoniq_ui::HarmoniqPalette;

use super::command_dispatch::CommandSender;

#[derive(Default, Debug, Clone)]
pub struct PluginsMenuState {
    pub scanner_open: bool,
    pub library_open: bool,
    pub plugin_manager_open: bool,
}

impl PluginsMenuState {
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        _commands: &CommandSender,
    ) {
        ui.menu_button("Plugins", |ui| {
            if ui
                .button(RichText::new("Add Plugins…").color(palette.text_primary))
                .clicked()
            {
                self.scanner_open = true;
                ui.close_menu();
            }
            if ui
                .button(RichText::new("Plugin Library…").color(palette.text_primary))
                .clicked()
            {
                self.library_open = true;
                ui.close_menu();
            }
            if ui
                .button(RichText::new("Plugin Manager…").color(palette.text_primary))
                .clicked()
            {
                self.plugin_manager_open = true;
                ui.close_menu();
            }
        });
    }
}
