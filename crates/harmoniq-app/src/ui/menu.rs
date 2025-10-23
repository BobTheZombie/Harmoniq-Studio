use std::path::PathBuf;

use egui::{self, RichText};

use harmoniq_ui::HarmoniqPalette;

use crate::ui::event_bus::{AppEvent, EventBus, LayoutEvent, TransportEvent};

#[derive(Default)]
pub struct MenuBarState {
    last_browser_visible: bool,
}

impl MenuBarState {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        event_bus: &EventBus,
        browser_visible: bool,
    ) {
        self.last_browser_visible = browser_visible;
        egui::menu::bar(ui, |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Harmoniq Studio")
                    .color(palette.accent)
                    .strong()
                    .size(16.0),
            );
            ui.add_space(18.0);

            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
                if ui.button("Open…").clicked() {
                    event_bus.publish(AppEvent::OpenFile(PathBuf::from("project.hst")));
                    ui.close_menu();
                }
                if ui.button("Save").clicked() {
                    event_bus.publish(AppEvent::SaveProject);
                    ui.close_menu();
                }
                if ui.button("Export Audio").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
                if ui.button("Preferences").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
            });

            ui.menu_button("Edit", |ui| {
                for action in ["Undo", "Redo", "Cut", "Copy", "Paste"] {
                    if ui.button(action).clicked() {
                        event_bus.publish(AppEvent::RequestRepaint);
                        ui.close_menu();
                    }
                }
            });

            ui.menu_button("View", |ui| {
                let mut browser_toggle = self.last_browser_visible;
                if ui.checkbox(&mut browser_toggle, "Browser").changed() {
                    event_bus.publish(AppEvent::Layout(LayoutEvent::ToggleBrowser));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Reset Layout").clicked() {
                    event_bus.publish(AppEvent::Layout(LayoutEvent::ResetWorkspace));
                    ui.close_menu();
                }
            });

            ui.menu_button("Options", |ui| {
                if ui.button("Audio Settings").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
                if ui.button("MIDI Settings").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
                if ui.button("Themes").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
            });

            ui.menu_button("Tools", |ui| {
                if ui.button("Plugin Manager").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
                if ui.button("Project Info").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
            });

            ui.menu_button("Help", |ui| {
                if ui.button("Manual").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
                if ui.button("About").clicked() {
                    event_bus.publish(AppEvent::RequestRepaint);
                    ui.close_menu();
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Stop").clicked() {
                    event_bus.publish(AppEvent::Transport(TransportEvent::Stop));
                }
                if ui.button("Play").clicked() {
                    event_bus.publish(AppEvent::Transport(TransportEvent::Play));
                }
            });
        });
    }
}
