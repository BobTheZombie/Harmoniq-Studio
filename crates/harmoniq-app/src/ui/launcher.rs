use eframe::egui::{self, RichText};
use harmoniq_ui::HarmoniqPalette;

use super::commands::Command;
use super::floating::FloatingKind;
use super::{command_dispatch::CommandSender, commands::FloatingCommand};
use crate::AppIcons;

pub const UID_MIXER: &str = "widget.mixer";
pub const UID_PLAYLIST: &str = "widget.playlist";
pub const UID_SEQUENCER: &str = "widget.sequencer";
pub const UID_PIANO_ROLL: &str = "widget.piano_roll";
pub const UID_BROWSER: &str = "widget.browser";
pub const UID_CHANNEL_RACK: &str = "widget.channel_rack";
pub const UID_CONSOLE: &str = "widget.console";
pub const UID_INSPECTOR: &str = "widget.inspector";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LauncherIcon {
    Mixer,
    Playlist,
    Sequencer,
    PianoRoll,
    Browser,
    ChannelRack,
    Console,
    Inspector,
}

#[derive(Debug, Clone)]
pub struct LauncherWidget {
    pub uid: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: LauncherIcon,
    pub default_size: egui::Vec2,
}

impl LauncherWidget {
    pub fn kind(&self) -> FloatingKind {
        FloatingKind::UiWidget {
            uid: self.uid.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct UiLauncher {
    widgets: Vec<LauncherWidget>,
}

impl UiLauncher {
    pub fn new() -> Self {
        Self {
            widgets: vec![
                LauncherWidget {
                    uid: UID_MIXER,
                    title: "Mixer",
                    description: "Balance inserts, buses, and metering",
                    icon: LauncherIcon::Mixer,
                    default_size: egui::vec2(860.0, 480.0),
                },
                LauncherWidget {
                    uid: UID_PLAYLIST,
                    title: "Playlist",
                    description: "Arrange clips and audio across tracks",
                    icon: LauncherIcon::Playlist,
                    default_size: egui::vec2(1040.0, 560.0),
                },
                LauncherWidget {
                    uid: UID_SEQUENCER,
                    title: "Sequencer",
                    description: "Program patterns, automation, and rhythm",
                    icon: LauncherIcon::Sequencer,
                    default_size: egui::vec2(1040.0, 560.0),
                },
                LauncherWidget {
                    uid: UID_PIANO_ROLL,
                    title: "Piano Roll",
                    description: "Deep MIDI editing with advanced tooling",
                    icon: LauncherIcon::PianoRoll,
                    default_size: egui::vec2(980.0, 560.0),
                },
                LauncherWidget {
                    uid: UID_BROWSER,
                    title: "Browser",
                    description: "Browse plugins, samples, and presets",
                    icon: LauncherIcon::Browser,
                    default_size: egui::vec2(420.0, 520.0),
                },
                LauncherWidget {
                    uid: UID_CHANNEL_RACK,
                    title: "Channel Rack",
                    description: "Trigger drum kits and instruments",
                    icon: LauncherIcon::ChannelRack,
                    default_size: egui::vec2(880.0, 420.0),
                },
                LauncherWidget {
                    uid: UID_CONSOLE,
                    title: "Console",
                    description: "System logs and debugging output",
                    icon: LauncherIcon::Console,
                    default_size: egui::vec2(720.0, 420.0),
                },
                LauncherWidget {
                    uid: UID_INSPECTOR,
                    title: "Inspector",
                    description: "Context-aware properties and routing",
                    icon: LauncherIcon::Inspector,
                    default_size: egui::vec2(560.0, 420.0),
                },
            ],
        }
    }

    pub fn widgets(&self) -> &[LauncherWidget] {
        &self.widgets
    }

    pub fn by_uid(&self, uid: &str) -> Option<&LauncherWidget> {
        self.widgets.iter().find(|widget| widget.uid == uid)
    }

    pub fn ui(
        &self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        icons: &AppIcons,
        commands: &CommandSender,
    ) {
        ui.vertical_centered(|ui| {
            ui.heading(RichText::new("UI Launcher").color(palette.text_primary));
            ui.label(
                RichText::new("Launch Harmoniq Studio workspaces as floating widgets.")
                    .color(palette.text_muted),
            );
            ui.add_space(12.0);
        });

        let columns = 2;
        let mut row_widgets = Vec::new();
        for widget in &self.widgets {
            row_widgets.push(widget);
            if row_widgets.len() == columns {
                self.render_row(ui, &row_widgets, palette, icons, commands);
                row_widgets.clear();
                ui.add_space(10.0);
            }
        }
        if !row_widgets.is_empty() {
            self.render_row(ui, &row_widgets, palette, icons, commands);
        }
    }

    fn render_row(
        &self,
        ui: &mut egui::Ui,
        widgets: &[&LauncherWidget],
        palette: &HarmoniqPalette,
        icons: &AppIcons,
        commands: &CommandSender,
    ) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
            for widget in widgets {
                let icon = icon_handle(widget.icon, icons);
                let card = egui::Frame::group(ui.style())
                    .fill(palette.panel)
                    .stroke(egui::Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::same(12.0));
                card.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.image((icon.id(), egui::vec2(28.0, 28.0)));
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(widget.title)
                                    .color(palette.text_primary)
                                    .strong(),
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(widget.description)
                                .color(palette.text_muted)
                                .size(13.0),
                        );
                        ui.add_space(10.0);
                        if ui
                            .add_sized(egui::vec2(160.0, 30.0), egui::Button::new("Launch"))
                            .clicked()
                        {
                            let _ = commands
                                .try_send(Command::Floating(FloatingCommand::Open(widget.kind())));
                        }
                    });
                });
            }
        });
    }
}

fn icon_handle<'a>(icon: LauncherIcon, icons: &'a AppIcons) -> &'a egui::TextureHandle {
    match icon {
        LauncherIcon::Mixer => &icons.mixer,
        LauncherIcon::Playlist => &icons.playlist,
        LauncherIcon::Sequencer => &icons.sequencer,
        LauncherIcon::PianoRoll => &icons.piano_roll,
        LauncherIcon::Browser => &icons.open,
        LauncherIcon::ChannelRack => &icons.track,
        LauncherIcon::Console => &icons.clip,
        LauncherIcon::Inspector => &icons.settings,
    }
}
