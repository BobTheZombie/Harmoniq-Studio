use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use super::floating::{FloatingKind, FloatingWindowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandId {
    FileNew,
    FileOpen,
    FileOpenRecent,
    FileSave,
    FileSaveAs,
    FileExport,
    FileCloseProject,
    EditUndo,
    EditRedo,
    EditCut,
    EditCopy,
    EditPaste,
    EditDelete,
    EditSelectAll,
    EditPreferences,
    ViewToggleMixer,
    ViewTogglePlaylist,
    ViewTogglePianoRoll,
    ViewToggleSequencer,
    ViewToggleBrowser,
    ViewTogglePerfHud,
    ViewZoomIn,
    ViewZoomOut,
    ViewToggleFullscreen,
    InsertAudioTrack,
    InsertMidiTrack,
    InsertReturnBus,
    InsertPluginInstrument,
    InsertPluginEffect,
    TrackArmRecord,
    TrackSolo,
    TrackMute,
    TrackFreezeCommit,
    TrackRename,
    TrackColor,
    MidiInputDevice,
    MidiChannel,
    MidiQuantize,
    MidiHumanize,
    MidiMetronomeSettings,
    TransportPlayPause,
    TransportStop,
    TransportRecord,
    TransportLoop,
    TransportLoopToSelection,
    TransportGoToStart,
    TransportTapTempo,
    OptionsAudioDevice,
    OptionsProjectSettings,
    OptionsThemeDark,
    OptionsThemeLight,
    OptionsThemeAuto,
    OptionsCpuMeter,
    OptionsMidiDevices,
    HelpAbout,
    HelpOpenLogs,
    HelpUserManual,
}

impl CommandId {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandId::FileNew => "file.new",
            CommandId::FileOpen => "file.open",
            CommandId::FileOpenRecent => "file.open_recent",
            CommandId::FileSave => "file.save",
            CommandId::FileSaveAs => "file.save_as",
            CommandId::FileExport => "file.export",
            CommandId::FileCloseProject => "file.close_project",
            CommandId::EditUndo => "edit.undo",
            CommandId::EditRedo => "edit.redo",
            CommandId::EditCut => "edit.cut",
            CommandId::EditCopy => "edit.copy",
            CommandId::EditPaste => "edit.paste",
            CommandId::EditDelete => "edit.delete",
            CommandId::EditSelectAll => "edit.select_all",
            CommandId::EditPreferences => "edit.preferences",
            CommandId::ViewToggleMixer => "view.toggle_mixer",
            CommandId::ViewTogglePlaylist => "view.toggle_playlist",
            CommandId::ViewTogglePianoRoll => "view.toggle_piano_roll",
            CommandId::ViewToggleSequencer => "view.toggle_sequencer",
            CommandId::ViewToggleBrowser => "view.toggle_browser",
            CommandId::ViewTogglePerfHud => "view.toggle_perf_hud",
            CommandId::ViewZoomIn => "view.zoom_in",
            CommandId::ViewZoomOut => "view.zoom_out",
            CommandId::ViewToggleFullscreen => "view.toggle_fullscreen",
            CommandId::InsertAudioTrack => "insert.audio_track",
            CommandId::InsertMidiTrack => "insert.midi_track",
            CommandId::InsertReturnBus => "insert.return_bus",
            CommandId::InsertPluginInstrument => "insert.plugin_instrument",
            CommandId::InsertPluginEffect => "insert.plugin_effect",
            CommandId::TrackArmRecord => "track.arm_record",
            CommandId::TrackSolo => "track.solo",
            CommandId::TrackMute => "track.mute",
            CommandId::TrackFreezeCommit => "track.freeze_commit",
            CommandId::TrackRename => "track.rename",
            CommandId::TrackColor => "track.color",
            CommandId::MidiInputDevice => "midi.input_device",
            CommandId::MidiChannel => "midi.channel",
            CommandId::MidiQuantize => "midi.quantize",
            CommandId::MidiHumanize => "midi.humanize",
            CommandId::MidiMetronomeSettings => "midi.metronome_settings",
            CommandId::TransportPlayPause => "transport.play_pause",
            CommandId::TransportStop => "transport.stop",
            CommandId::TransportRecord => "transport.record",
            CommandId::TransportLoop => "transport.loop",
            CommandId::TransportLoopToSelection => "transport.loop_to_selection",
            CommandId::TransportGoToStart => "transport.go_to_start",
            CommandId::TransportTapTempo => "transport.tap_tempo",
            CommandId::OptionsAudioDevice => "options.audio_device",
            CommandId::OptionsProjectSettings => "options.project_settings",
            CommandId::OptionsThemeDark => "options.theme_dark",
            CommandId::OptionsThemeLight => "options.theme_light",
            CommandId::OptionsThemeAuto => "options.theme_auto",
            CommandId::OptionsCpuMeter => "options.cpu_meter",
            CommandId::OptionsMidiDevices => "options.midi_devices",
            CommandId::HelpAbout => "help.about",
            CommandId::HelpOpenLogs => "help.open_logs",
            CommandId::HelpUserManual => "help.user_manual",
        }
    }
}

impl FromStr for CommandId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "file.new" => Ok(CommandId::FileNew),
            "file.open" => Ok(CommandId::FileOpen),
            "file.open_recent" => Ok(CommandId::FileOpenRecent),
            "file.save" => Ok(CommandId::FileSave),
            "file.save_as" => Ok(CommandId::FileSaveAs),
            "file.export" => Ok(CommandId::FileExport),
            "file.close_project" => Ok(CommandId::FileCloseProject),
            "edit.undo" => Ok(CommandId::EditUndo),
            "edit.redo" => Ok(CommandId::EditRedo),
            "edit.cut" => Ok(CommandId::EditCut),
            "edit.copy" => Ok(CommandId::EditCopy),
            "edit.paste" => Ok(CommandId::EditPaste),
            "edit.delete" => Ok(CommandId::EditDelete),
            "edit.select_all" => Ok(CommandId::EditSelectAll),
            "edit.preferences" => Ok(CommandId::EditPreferences),
            "view.toggle_mixer" => Ok(CommandId::ViewToggleMixer),
            "view.toggle_playlist" => Ok(CommandId::ViewTogglePlaylist),
            "view.toggle_piano_roll" => Ok(CommandId::ViewTogglePianoRoll),
            "view.toggle_sequencer" => Ok(CommandId::ViewToggleSequencer),
            "view.toggle_browser" => Ok(CommandId::ViewToggleBrowser),
            "view.toggle_perf_hud" => Ok(CommandId::ViewTogglePerfHud),
            "view.zoom_in" => Ok(CommandId::ViewZoomIn),
            "view.zoom_out" => Ok(CommandId::ViewZoomOut),
            "view.toggle_fullscreen" => Ok(CommandId::ViewToggleFullscreen),
            "insert.audio_track" => Ok(CommandId::InsertAudioTrack),
            "insert.midi_track" => Ok(CommandId::InsertMidiTrack),
            "insert.return_bus" => Ok(CommandId::InsertReturnBus),
            "insert.plugin_instrument" => Ok(CommandId::InsertPluginInstrument),
            "insert.plugin_effect" => Ok(CommandId::InsertPluginEffect),
            "track.arm_record" => Ok(CommandId::TrackArmRecord),
            "track.solo" => Ok(CommandId::TrackSolo),
            "track.mute" => Ok(CommandId::TrackMute),
            "track.freeze_commit" => Ok(CommandId::TrackFreezeCommit),
            "track.rename" => Ok(CommandId::TrackRename),
            "track.color" => Ok(CommandId::TrackColor),
            "midi.input_device" => Ok(CommandId::MidiInputDevice),
            "midi.channel" => Ok(CommandId::MidiChannel),
            "midi.quantize" => Ok(CommandId::MidiQuantize),
            "midi.humanize" => Ok(CommandId::MidiHumanize),
            "midi.metronome_settings" => Ok(CommandId::MidiMetronomeSettings),
            "transport.play_pause" => Ok(CommandId::TransportPlayPause),
            "transport.stop" => Ok(CommandId::TransportStop),
            "transport.record" => Ok(CommandId::TransportRecord),
            "transport.loop" => Ok(CommandId::TransportLoop),
            "transport.loop_to_selection" => Ok(CommandId::TransportLoopToSelection),
            "transport.go_to_start" => Ok(CommandId::TransportGoToStart),
            "transport.tap_tempo" => Ok(CommandId::TransportTapTempo),
            "options.audio_device" => Ok(CommandId::OptionsAudioDevice),
            "options.project_settings" => Ok(CommandId::OptionsProjectSettings),
            "options.theme_dark" => Ok(CommandId::OptionsThemeDark),
            "options.theme_light" => Ok(CommandId::OptionsThemeLight),
            "options.theme_auto" => Ok(CommandId::OptionsThemeAuto),
            "options.cpu_meter" => Ok(CommandId::OptionsCpuMeter),
            "options.midi_devices" => Ok(CommandId::OptionsMidiDevices),
            "help.about" => Ok(CommandId::HelpAbout),
            "help.open_logs" => Ok(CommandId::HelpOpenLogs),
            "help.user_manual" => Ok(CommandId::HelpUserManual),
            _ => Err("unknown command id"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    File(FileCommand),
    Edit(EditCommand),
    View(ViewCommand),
    Insert(InsertCommand),
    Track(TrackCommand),
    Midi(MidiCommand),
    Transport(TransportCommand),
    Options(OptionsCommand),
    Help(HelpCommand),
    Floating(FloatingCommand),
}

impl Command {
    pub fn id(&self) -> CommandId {
        match self {
            Command::File(cmd) => cmd.id(),
            Command::Edit(cmd) => cmd.id(),
            Command::View(cmd) => cmd.id(),
            Command::Insert(cmd) => cmd.id(),
            Command::Track(cmd) => cmd.id(),
            Command::Midi(cmd) => cmd.id(),
            Command::Transport(cmd) => cmd.id(),
            Command::Options(cmd) => cmd.id(),
            Command::Help(cmd) => cmd.id(),
            Command::Floating(_) => panic!("floating commands do not have a CommandId"),
        }
    }

    pub fn from_id(id: CommandId) -> Option<Self> {
        Some(match id {
            CommandId::FileNew => Command::File(FileCommand::New),
            CommandId::FileOpen => Command::File(FileCommand::Open),
            CommandId::FileOpenRecent => return None,
            CommandId::FileSave => Command::File(FileCommand::Save),
            CommandId::FileSaveAs => Command::File(FileCommand::SaveAs),
            CommandId::FileExport => Command::File(FileCommand::Export),
            CommandId::FileCloseProject => Command::File(FileCommand::CloseProject),
            CommandId::EditUndo => Command::Edit(EditCommand::Undo),
            CommandId::EditRedo => Command::Edit(EditCommand::Redo),
            CommandId::EditCut => Command::Edit(EditCommand::Cut),
            CommandId::EditCopy => Command::Edit(EditCommand::Copy),
            CommandId::EditPaste => Command::Edit(EditCommand::Paste),
            CommandId::EditDelete => Command::Edit(EditCommand::Delete),
            CommandId::EditSelectAll => Command::Edit(EditCommand::SelectAll),
            CommandId::EditPreferences => Command::Edit(EditCommand::Preferences),
            CommandId::ViewToggleMixer => Command::View(ViewCommand::ToggleMixer),
            CommandId::ViewTogglePlaylist => Command::View(ViewCommand::TogglePlaylist),
            CommandId::ViewTogglePianoRoll => Command::View(ViewCommand::TogglePianoRoll),
            CommandId::ViewToggleSequencer => Command::View(ViewCommand::ToggleSequencer),
            CommandId::ViewToggleBrowser => Command::View(ViewCommand::ToggleBrowser),
            CommandId::ViewTogglePerfHud => Command::View(ViewCommand::TogglePerfHud),
            CommandId::ViewZoomIn => Command::View(ViewCommand::ZoomIn),
            CommandId::ViewZoomOut => Command::View(ViewCommand::ZoomOut),
            CommandId::ViewToggleFullscreen => Command::View(ViewCommand::ToggleFullscreen),
            CommandId::InsertAudioTrack => Command::Insert(InsertCommand::AudioTrack),
            CommandId::InsertMidiTrack => Command::Insert(InsertCommand::MidiTrack),
            CommandId::InsertReturnBus => Command::Insert(InsertCommand::ReturnBus),
            CommandId::InsertPluginInstrument => Command::Insert(
                InsertCommand::AddPluginOnSelectedTrack(PluginCategory::Instrument),
            ),
            CommandId::InsertPluginEffect => Command::Insert(
                InsertCommand::AddPluginOnSelectedTrack(PluginCategory::Effect),
            ),
            CommandId::TrackArmRecord => Command::Track(TrackCommand::ArmRecord),
            CommandId::TrackSolo => Command::Track(TrackCommand::Solo),
            CommandId::TrackMute => Command::Track(TrackCommand::Mute),
            CommandId::TrackFreezeCommit => Command::Track(TrackCommand::FreezeCommit),
            CommandId::TrackRename => Command::Track(TrackCommand::Rename),
            CommandId::TrackColor => Command::Track(TrackCommand::Color),
            CommandId::MidiInputDevice => Command::Midi(MidiCommand::OpenInputDevicePicker),
            CommandId::MidiChannel => Command::Midi(MidiCommand::OpenChannelPicker),
            CommandId::MidiQuantize => Command::Midi(MidiCommand::Quantize),
            CommandId::MidiHumanize => Command::Midi(MidiCommand::Humanize),
            CommandId::MidiMetronomeSettings => Command::Midi(MidiCommand::MetronomeSettings),
            CommandId::TransportPlayPause => Command::Transport(TransportCommand::TogglePlayPause),
            CommandId::TransportStop => Command::Transport(TransportCommand::Stop),
            CommandId::TransportRecord => Command::Transport(TransportCommand::RecordArm),
            CommandId::TransportLoop => Command::Transport(TransportCommand::ToggleLoop),
            CommandId::TransportLoopToSelection => {
                Command::Transport(TransportCommand::LoopToSelection)
            }
            CommandId::TransportGoToStart => Command::Transport(TransportCommand::GoToStart),
            CommandId::TransportTapTempo => Command::Transport(TransportCommand::TapTempo),
            CommandId::OptionsAudioDevice => Command::Options(OptionsCommand::AudioDeviceDialog),
            CommandId::OptionsProjectSettings => Command::Options(OptionsCommand::ProjectSettings),
            CommandId::OptionsThemeDark => Command::Options(OptionsCommand::Theme(ThemeMode::Dark)),
            CommandId::OptionsThemeLight => {
                Command::Options(OptionsCommand::Theme(ThemeMode::Light))
            }
            CommandId::OptionsThemeAuto => Command::Options(OptionsCommand::Theme(ThemeMode::Auto)),
            CommandId::OptionsCpuMeter => Command::Options(OptionsCommand::CpuMeter),
            CommandId::OptionsMidiDevices => Command::Options(OptionsCommand::MidiDevices),
            CommandId::HelpAbout => Command::Help(HelpCommand::About),
            CommandId::HelpOpenLogs => Command::Help(HelpCommand::OpenLogsFolder),
            CommandId::HelpUserManual => Command::Help(HelpCommand::UserManual),
        })
    }
}

#[derive(Debug, Clone)]
pub enum FileCommand {
    New,
    Open,
    OpenRecent(PathBuf),
    Save,
    SaveAs,
    Export,
    CloseProject,
}

impl FileCommand {
    pub fn id(&self) -> CommandId {
        match self {
            FileCommand::New => CommandId::FileNew,
            FileCommand::Open => CommandId::FileOpen,
            FileCommand::OpenRecent(_) => CommandId::FileOpenRecent,
            FileCommand::Save => CommandId::FileSave,
            FileCommand::SaveAs => CommandId::FileSaveAs,
            FileCommand::Export => CommandId::FileExport,
            FileCommand::CloseProject => CommandId::FileCloseProject,
        }
    }
}

#[derive(Debug, Clone)]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    Preferences,
}

impl EditCommand {
    pub fn id(&self) -> CommandId {
        match self {
            EditCommand::Undo => CommandId::EditUndo,
            EditCommand::Redo => CommandId::EditRedo,
            EditCommand::Cut => CommandId::EditCut,
            EditCommand::Copy => CommandId::EditCopy,
            EditCommand::Paste => CommandId::EditPaste,
            EditCommand::Delete => CommandId::EditDelete,
            EditCommand::SelectAll => CommandId::EditSelectAll,
            EditCommand::Preferences => CommandId::EditPreferences,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ViewCommand {
    ToggleMixer,
    TogglePlaylist,
    TogglePianoRoll,
    ToggleSequencer,
    ToggleBrowser,
    TogglePerfHud,
    ZoomIn,
    ZoomOut,
    ToggleFullscreen,
}

impl ViewCommand {
    pub fn id(&self) -> CommandId {
        match self {
            ViewCommand::ToggleMixer => CommandId::ViewToggleMixer,
            ViewCommand::TogglePlaylist => CommandId::ViewTogglePlaylist,
            ViewCommand::TogglePianoRoll => CommandId::ViewTogglePianoRoll,
            ViewCommand::ToggleSequencer => CommandId::ViewToggleSequencer,
            ViewCommand::ToggleBrowser => CommandId::ViewToggleBrowser,
            ViewCommand::TogglePerfHud => CommandId::ViewTogglePerfHud,
            ViewCommand::ZoomIn => CommandId::ViewZoomIn,
            ViewCommand::ZoomOut => CommandId::ViewZoomOut,
            ViewCommand::ToggleFullscreen => CommandId::ViewToggleFullscreen,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PluginCategory {
    Instrument,
    Effect,
}

impl fmt::Display for PluginCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginCategory::Instrument => write!(f, "Instrument"),
            PluginCategory::Effect => write!(f, "Effect"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum InsertCommand {
    AudioTrack,
    MidiTrack,
    ReturnBus,
    AddPluginOnSelectedTrack(PluginCategory),
}

impl InsertCommand {
    pub fn id(&self) -> CommandId {
        match self {
            InsertCommand::AudioTrack => CommandId::InsertAudioTrack,
            InsertCommand::MidiTrack => CommandId::InsertMidiTrack,
            InsertCommand::ReturnBus => CommandId::InsertReturnBus,
            InsertCommand::AddPluginOnSelectedTrack(category) => match category {
                PluginCategory::Instrument => CommandId::InsertPluginInstrument,
                PluginCategory::Effect => CommandId::InsertPluginEffect,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum TrackCommand {
    ArmRecord,
    Solo,
    Mute,
    FreezeCommit,
    Rename,
    Color,
}

impl TrackCommand {
    pub fn id(&self) -> CommandId {
        match self {
            TrackCommand::ArmRecord => CommandId::TrackArmRecord,
            TrackCommand::Solo => CommandId::TrackSolo,
            TrackCommand::Mute => CommandId::TrackMute,
            TrackCommand::FreezeCommit => CommandId::TrackFreezeCommit,
            TrackCommand::Rename => CommandId::TrackRename,
            TrackCommand::Color => CommandId::TrackColor,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MidiCommand {
    OpenInputDevicePicker,
    SelectInputDevice(String),
    OpenChannelPicker,
    SelectChannel(u8),
    Quantize,
    Humanize,
    MetronomeSettings,
}

impl MidiCommand {
    pub fn id(&self) -> CommandId {
        match self {
            MidiCommand::OpenInputDevicePicker => CommandId::MidiInputDevice,
            MidiCommand::SelectInputDevice(_) => CommandId::MidiInputDevice,
            MidiCommand::OpenChannelPicker => CommandId::MidiChannel,
            MidiCommand::SelectChannel(_) => CommandId::MidiChannel,
            MidiCommand::Quantize => CommandId::MidiQuantize,
            MidiCommand::Humanize => CommandId::MidiHumanize,
            MidiCommand::MetronomeSettings => CommandId::MidiMetronomeSettings,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransportCommand {
    TogglePlayPause,
    Stop,
    RecordArm,
    ToggleLoop,
    LoopToSelection,
    GoToStart,
    TapTempo,
}

impl TransportCommand {
    pub fn id(&self) -> CommandId {
        match self {
            TransportCommand::TogglePlayPause => CommandId::TransportPlayPause,
            TransportCommand::Stop => CommandId::TransportStop,
            TransportCommand::RecordArm => CommandId::TransportRecord,
            TransportCommand::ToggleLoop => CommandId::TransportLoop,
            TransportCommand::LoopToSelection => CommandId::TransportLoopToSelection,
            TransportCommand::GoToStart => CommandId::TransportGoToStart,
            TransportCommand::TapTempo => CommandId::TransportTapTempo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    Dark,
    Light,
    Auto,
}

#[derive(Debug, Clone)]
pub enum OptionsCommand {
    AudioDeviceDialog,
    ProjectSettings,
    Theme(ThemeMode),
    CpuMeter,
    MidiDevices,
}

impl OptionsCommand {
    pub fn id(&self) -> CommandId {
        match self {
            OptionsCommand::AudioDeviceDialog => CommandId::OptionsAudioDevice,
            OptionsCommand::ProjectSettings => CommandId::OptionsProjectSettings,
            OptionsCommand::Theme(ThemeMode::Dark) => CommandId::OptionsThemeDark,
            OptionsCommand::Theme(ThemeMode::Light) => CommandId::OptionsThemeLight,
            OptionsCommand::Theme(ThemeMode::Auto) => CommandId::OptionsThemeAuto,
            OptionsCommand::CpuMeter => CommandId::OptionsCpuMeter,
            OptionsCommand::MidiDevices => CommandId::OptionsMidiDevices,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HelpCommand {
    About,
    OpenLogsFolder,
    UserManual,
}

impl HelpCommand {
    pub fn id(&self) -> CommandId {
        match self {
            HelpCommand::About => CommandId::HelpAbout,
            HelpCommand::OpenLogsFolder => CommandId::HelpOpenLogs,
            HelpCommand::UserManual => CommandId::HelpUserManual,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FloatingCommand {
    Open(FloatingKind),
    Close(FloatingWindowId),
    Toggle(FloatingKind),
    Focus(FloatingWindowId),
    CloseAll(FloatingKind),
}
