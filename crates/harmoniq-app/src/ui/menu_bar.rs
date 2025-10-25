use std::path::PathBuf;

use eframe::egui::{self, RichText};
use harmoniq_ui::HarmoniqPalette;

use super::command_dispatch::CommandSender;
use super::commands::{
    Command, CommandId, FileCommand, FloatingCommand, HelpCommand, InsertCommand, MidiCommand,
    OptionsCommand, PluginCategory, ThemeMode, TrackCommand, TransportCommand, ViewCommand,
};
use super::floating::FloatingKind;
use super::menu_plugins::PluginsMenuState;
use super::shortcuts::ShortcutMap;

#[derive(Debug, Clone)]
pub struct MenuBarState {
    pub plugins_menu: PluginsMenuState,
}

impl Default for MenuBarState {
    fn default() -> Self {
        Self {
            plugins_menu: PluginsMenuState::default(),
        }
    }
}

pub struct MenuBarSnapshot<'a> {
    pub mixer_visible: bool,
    pub piano_roll_visible: bool,
    pub browser_visible: bool,
    pub fullscreen: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub transport_playing: bool,
    pub transport_record_armed: bool,
    pub transport_loop_enabled: bool,
    pub recent_projects: &'a [PathBuf],
    pub midi_inputs: &'a [String],
    pub selected_midi_input: Option<&'a str>,
    pub midi_channel: u8,
}

impl Default for MenuBarSnapshot<'_> {
    fn default() -> Self {
        Self {
            mixer_visible: true,
            piano_roll_visible: true,
            browser_visible: true,
            fullscreen: false,
            can_undo: false,
            can_redo: false,
            transport_playing: false,
            transport_record_armed: false,
            transport_loop_enabled: false,
            recent_projects: &[],
            midi_inputs: &[],
            selected_midi_input: None,
            midi_channel: 1,
        }
    }
}

impl MenuBarState {
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        egui::menu::bar(ui, |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Harmoniq Studio")
                    .color(palette.accent)
                    .strong()
                    .size(16.0),
            );
            ui.add_space(18.0);

            self.file_menu(ui, shortcuts, commands, snapshot);
            self.edit_menu(ui, shortcuts, commands, snapshot);
            self.view_menu(ui, shortcuts, commands, snapshot);
            self.insert_menu(ui, shortcuts, commands);
            self.track_menu(ui, shortcuts, commands, snapshot);
            self.midi_menu(ui, shortcuts, commands, snapshot);
            self.transport_menu(ui, shortcuts, commands, snapshot);
            self.plugins_menu.render(ui, palette, commands);
            self.options_menu(ui, shortcuts, commands, snapshot);
            self.help_menu(ui, commands);
        });
    }

    fn file_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("File", |ui| {
            if menu_item(ui, "New", CommandId::FileNew, shortcuts) {
                let _ = commands.try_send(Command::File(FileCommand::New));
                ui.close_menu();
            }
            if menu_item(ui, "Open…", CommandId::FileOpen, shortcuts) {
                let _ = commands.try_send(Command::File(FileCommand::Open));
                ui.close_menu();
            }
            ui.menu_button("Open Recent", |ui| {
                if snapshot.recent_projects.is_empty() {
                    ui.label(RichText::new("No recent projects").italics());
                } else {
                    for path in snapshot.recent_projects {
                        let label = path
                            .file_name()
                            .and_then(|name| name.to_str().map(|name| name.to_owned()))
                            .unwrap_or_else(|| path.to_string_lossy().into_owned());
                        if ui.button(label.clone()).clicked() {
                            let _ = commands
                                .try_send(Command::File(FileCommand::OpenRecent(path.clone())));
                            ui.close_menu();
                        }
                    }
                }
            });
            ui.separator();
            if menu_item(ui, "Save", CommandId::FileSave, shortcuts) {
                let _ = commands.try_send(Command::File(FileCommand::Save));
                ui.close_menu();
            }
            if menu_item(ui, "Save As…", CommandId::FileSaveAs, shortcuts) {
                let _ = commands.try_send(Command::File(FileCommand::SaveAs));
                ui.close_menu();
            }
            if menu_item(ui, "Export/Render…", CommandId::FileExport, shortcuts) {
                let _ = commands.try_send(Command::File(FileCommand::Export));
                ui.close_menu();
            }
            ui.separator();
            if menu_item(ui, "Close Project", CommandId::FileCloseProject, shortcuts) {
                let _ = commands.try_send(Command::File(FileCommand::CloseProject));
                ui.close_menu();
            }
        });
    }

    fn edit_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("Edit", |ui| {
            if menu_item_enabled(
                ui,
                "Undo",
                CommandId::EditUndo,
                shortcuts,
                snapshot.can_undo,
            ) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Undo));
                ui.close_menu();
            }
            if menu_item_enabled(
                ui,
                "Redo",
                CommandId::EditRedo,
                shortcuts,
                snapshot.can_redo,
            ) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Redo));
                ui.close_menu();
            }
            ui.separator();
            if menu_item(ui, "Cut", CommandId::EditCut, shortcuts) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Cut));
                ui.close_menu();
            }
            if menu_item(ui, "Copy", CommandId::EditCopy, shortcuts) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Copy));
                ui.close_menu();
            }
            if menu_item(ui, "Paste", CommandId::EditPaste, shortcuts) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Paste));
                ui.close_menu();
            }
            if menu_item(ui, "Delete", CommandId::EditDelete, shortcuts) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Delete));
                ui.close_menu();
            }
            ui.separator();
            if menu_item(ui, "Select All", CommandId::EditSelectAll, shortcuts) {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::SelectAll));
                ui.close_menu();
            }
            if ui
                .button(with_shortcut(
                    "Preferences/Settings…",
                    shortcuts,
                    CommandId::EditPreferences,
                ))
                .clicked()
            {
                let _ = commands.try_send(Command::Edit(super::commands::EditCommand::Preferences));
                ui.close_menu();
            }
        });
    }

    fn view_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("View", |ui| {
            let mut mixer = snapshot.mixer_visible;
            if toggle_item(
                ui,
                "Toggle Mixer",
                CommandId::ViewToggleMixer,
                shortcuts,
                &mut mixer,
            ) {
                let _ = commands.try_send(Command::View(ViewCommand::ToggleMixer));
                ui.close_menu();
            }
            let mut piano_roll = snapshot.piano_roll_visible;
            if toggle_item(
                ui,
                "Toggle Piano Roll",
                CommandId::ViewTogglePianoRoll,
                shortcuts,
                &mut piano_roll,
            ) {
                let _ = commands.try_send(Command::View(ViewCommand::TogglePianoRoll));
                ui.close_menu();
            }
            let mut browser = snapshot.browser_visible;
            if toggle_item(
                ui,
                "Toggle Browser",
                CommandId::ViewToggleBrowser,
                shortcuts,
                &mut browser,
            ) {
                let _ = commands.try_send(Command::View(ViewCommand::ToggleBrowser));
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Piano Roll (Floating)").clicked() {
                let _ = commands.try_send(Command::Floating(FloatingCommand::Open(
                    FloatingKind::PianoRoll { track_id: 0 },
                )));
                ui.close_menu();
            }
            if ui.button("Mixer Inspector (Floating)").clicked() {
                let _ = commands.try_send(Command::Floating(FloatingCommand::Open(
                    FloatingKind::MixerInsert { insert_idx: 0 },
                )));
                ui.close_menu();
            }
            if ui.button("Channel Inspector (Floating)").clicked() {
                let _ = commands.try_send(Command::Floating(FloatingCommand::Open(
                    FloatingKind::Inspector { selection: None },
                )));
                ui.close_menu();
            }
            if ui.button("MIDI Monitor (Floating)").clicked() {
                let _ = commands.try_send(Command::Floating(FloatingCommand::Open(
                    FloatingKind::MidiMonitor,
                )));
                ui.close_menu();
            }
            if ui.button("Performance Panel (Floating)").clicked() {
                let _ = commands.try_send(Command::Floating(FloatingCommand::Open(
                    FloatingKind::Performance,
                )));
                ui.close_menu();
            }
            ui.separator();
            if menu_item(ui, "Zoom In", CommandId::ViewZoomIn, shortcuts) {
                let _ = commands.try_send(Command::View(ViewCommand::ZoomIn));
                ui.close_menu();
            }
            if menu_item(ui, "Zoom Out", CommandId::ViewZoomOut, shortcuts) {
                let _ = commands.try_send(Command::View(ViewCommand::ZoomOut));
                ui.close_menu();
            }
            let mut fullscreen = snapshot.fullscreen;
            if toggle_item(
                ui,
                "Toggle Fullscreen",
                CommandId::ViewToggleFullscreen,
                shortcuts,
                &mut fullscreen,
            ) {
                let _ = commands.try_send(Command::View(ViewCommand::ToggleFullscreen));
                ui.close_menu();
            }
        });
    }

    fn insert_menu(
        &mut self,
        ui: &mut egui::Ui,
        _shortcuts: &ShortcutMap,
        commands: &CommandSender,
    ) {
        ui.menu_button("Insert", |ui| {
            if ui.button("Audio Track").clicked() {
                let _ = commands.try_send(Command::Insert(InsertCommand::AudioTrack));
                ui.close_menu();
            }
            if ui.button("MIDI Track").clicked() {
                let _ = commands.try_send(Command::Insert(InsertCommand::MidiTrack));
                ui.close_menu();
            }
            if ui.button("Return/Aux Bus").clicked() {
                let _ = commands.try_send(Command::Insert(InsertCommand::ReturnBus));
                ui.close_menu();
            }
            ui.menu_button("Add Plugin on Selected Track", |ui| {
                if ui.button("Instruments").clicked() {
                    let _ = commands.try_send(Command::Insert(
                        InsertCommand::AddPluginOnSelectedTrack(PluginCategory::Instrument),
                    ));
                    ui.close_menu();
                }
                if ui.button("Effects").clicked() {
                    let _ = commands.try_send(Command::Insert(
                        InsertCommand::AddPluginOnSelectedTrack(PluginCategory::Effect),
                    ));
                    ui.close_menu();
                }
            });
        });
    }

    fn track_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("Track", |ui| {
            let mut armed = snapshot.transport_record_armed;
            if toggle_item(
                ui,
                "Arm Record",
                CommandId::TrackArmRecord,
                shortcuts,
                &mut armed,
            ) {
                let _ = commands.try_send(Command::Track(TrackCommand::ArmRecord));
                ui.close_menu();
            }
            let mut solo = false;
            if toggle_item(ui, "Solo", CommandId::TrackSolo, shortcuts, &mut solo) {
                let _ = commands.try_send(Command::Track(TrackCommand::Solo));
                ui.close_menu();
            }
            let mut mute = false;
            if toggle_item(ui, "Mute", CommandId::TrackMute, shortcuts, &mut mute) {
                let _ = commands.try_send(Command::Track(TrackCommand::Mute));
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Freeze/Commit").clicked() {
                let _ = commands.try_send(Command::Track(TrackCommand::FreezeCommit));
                ui.close_menu();
            }
            if ui.button("Rename…").clicked() {
                let _ = commands.try_send(Command::Track(TrackCommand::Rename));
                ui.close_menu();
            }
            if ui.button("Color…").clicked() {
                let _ = commands.try_send(Command::Track(TrackCommand::Color));
                ui.close_menu();
            }
        });
    }

    fn midi_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("MIDI", |ui| {
            ui.menu_button("Input Device", |ui| {
                if snapshot.midi_inputs.is_empty() {
                    if ui.button("Configure Inputs…").clicked() {
                        let _ =
                            commands.try_send(Command::Midi(MidiCommand::OpenInputDevicePicker));
                        ui.close_menu();
                    }
                } else {
                    for device in snapshot.midi_inputs {
                        let selected = snapshot
                            .selected_midi_input
                            .map(|name| name == device.as_str())
                            .unwrap_or(false);
                        if ui.selectable_label(selected, device).clicked() {
                            let _ = commands.try_send(Command::Midi(
                                MidiCommand::SelectInputDevice(device.clone()),
                            ));
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Manage Devices…").clicked() {
                        let _ =
                            commands.try_send(Command::Midi(MidiCommand::OpenInputDevicePicker));
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("Channel", |ui| {
                for channel in 1..=16 {
                    let selected = snapshot.midi_channel == channel;
                    let label = format!("Channel {channel}");
                    if ui.selectable_label(selected, label).clicked() {
                        let _ =
                            commands.try_send(Command::Midi(MidiCommand::SelectChannel(channel)));
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            if menu_item(ui, "Quantize", CommandId::MidiQuantize, shortcuts) {
                let _ = commands.try_send(Command::Midi(MidiCommand::Quantize));
                ui.close_menu();
            }
            if ui.button("Humanize…").clicked() {
                let _ = commands.try_send(Command::Midi(MidiCommand::Humanize));
                ui.close_menu();
            }
            if ui.button("Metronome Settings…").clicked() {
                let _ = commands.try_send(Command::Midi(MidiCommand::MetronomeSettings));
                ui.close_menu();
            }
        });
    }

    fn transport_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("Transport", |ui| {
            if menu_item(
                ui,
                if snapshot.transport_playing {
                    "Pause"
                } else {
                    "Play"
                },
                CommandId::TransportPlayPause,
                shortcuts,
            ) {
                let _ = commands.try_send(Command::Transport(TransportCommand::TogglePlayPause));
                ui.close_menu();
            }
            if menu_item(ui, "Stop", CommandId::TransportStop, shortcuts) {
                let _ = commands.try_send(Command::Transport(TransportCommand::Stop));
                ui.close_menu();
            }
            let mut record = snapshot.transport_record_armed;
            if toggle_item(
                ui,
                "Record",
                CommandId::TransportRecord,
                shortcuts,
                &mut record,
            ) {
                let _ = commands.try_send(Command::Transport(TransportCommand::RecordArm));
                ui.close_menu();
            }
            let mut loop_enabled = snapshot.transport_loop_enabled;
            if toggle_item(
                ui,
                "Loop",
                CommandId::TransportLoop,
                shortcuts,
                &mut loop_enabled,
            ) {
                let _ = commands.try_send(Command::Transport(TransportCommand::ToggleLoop));
                ui.close_menu();
            }
            if ui.button("Set Loop to Selection").clicked() {
                let _ = commands.try_send(Command::Transport(TransportCommand::LoopToSelection));
                ui.close_menu();
            }
            if menu_item(ui, "Go to Start", CommandId::TransportGoToStart, shortcuts) {
                let _ = commands.try_send(Command::Transport(TransportCommand::GoToStart));
                ui.close_menu();
            }
            if menu_item(ui, "Tap Tempo", CommandId::TransportTapTempo, shortcuts) {
                let _ = commands.try_send(Command::Transport(TransportCommand::TapTempo));
                ui.close_menu();
            }
        });
    }

    fn options_menu(
        &mut self,
        ui: &mut egui::Ui,
        shortcuts: &ShortcutMap,
        commands: &CommandSender,
        _snapshot: &MenuBarSnapshot,
    ) {
        ui.menu_button("Options", |ui| {
            if ui
                .button(with_shortcut(
                    "Audio Device…",
                    shortcuts,
                    CommandId::OptionsAudioDevice,
                ))
                .clicked()
            {
                let _ = commands.try_send(Command::Options(OptionsCommand::AudioDeviceDialog));
                ui.close_menu();
            }
            if ui
                .button(with_shortcut(
                    "Project Settings…",
                    shortcuts,
                    CommandId::OptionsProjectSettings,
                ))
                .clicked()
            {
                let _ = commands.try_send(Command::Options(OptionsCommand::ProjectSettings));
                ui.close_menu();
            }
            ui.menu_button("Theme", |ui| {
                theme_option(ui, commands, ThemeMode::Dark);
                theme_option(ui, commands, ThemeMode::Light);
                theme_option(ui, commands, ThemeMode::Auto);
            });
            if ui.button("CPU Meter").clicked() {
                let _ = commands.try_send(Command::Options(OptionsCommand::CpuMeter));
                ui.close_menu();
            }
        });
    }

    fn help_menu(&mut self, ui: &mut egui::Ui, commands: &CommandSender) {
        ui.menu_button("Help", |ui| {
            if ui.button("About").clicked() {
                let _ = commands.try_send(Command::Help(HelpCommand::About));
                ui.close_menu();
            }
            if ui.button("Open Logs Folder").clicked() {
                let _ = commands.try_send(Command::Help(HelpCommand::OpenLogsFolder));
                ui.close_menu();
            }
            if ui.button("User Manual").clicked() {
                let _ = commands.try_send(Command::Help(HelpCommand::UserManual));
                ui.close_menu();
            }
        });
    }
}

fn theme_option(ui: &mut egui::Ui, commands: &CommandSender, mode: ThemeMode) {
    let label = match mode {
        ThemeMode::Dark => "Dark",
        ThemeMode::Light => "Light",
        ThemeMode::Auto => "Auto",
    };
    if ui.button(label).clicked() {
        let _ = commands.try_send(Command::Options(OptionsCommand::Theme(mode)));
        ui.close_menu();
    }
}

fn menu_item(ui: &mut egui::Ui, label: &str, id: CommandId, shortcuts: &ShortcutMap) -> bool {
    let label = with_shortcut(label, shortcuts, id);
    ui.button(label).clicked()
}

fn menu_item_enabled(
    ui: &mut egui::Ui,
    label: &str,
    id: CommandId,
    shortcuts: &ShortcutMap,
    enabled: bool,
) -> bool {
    let label = with_shortcut(label, shortcuts, id);
    ui.add_enabled(enabled, egui::Button::new(label)).clicked()
}

fn toggle_item(
    ui: &mut egui::Ui,
    label: &str,
    id: CommandId,
    shortcuts: &ShortcutMap,
    state: &mut bool,
) -> bool {
    let label = with_shortcut(label, shortcuts, id);
    let response = ui.checkbox(state, label);
    response.changed()
}

fn with_shortcut(label: &str, shortcuts: &ShortcutMap, id: CommandId) -> String {
    if let Some(shortcut) = shortcuts.label_for(id) {
        format!("{label}\t{shortcut}")
    } else {
        label.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_menu_state_defaults_closed() {
        let state = MenuBarState::default();
        assert!(!state.plugins_menu.scanner_open);
        assert!(!state.plugins_menu.library_open);
        assert!(!state.plugins_menu.plugin_manager_open);
    }
}
