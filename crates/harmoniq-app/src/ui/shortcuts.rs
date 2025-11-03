use std::collections::HashMap;

use eframe::egui::{self, Context, Key};

use super::command_dispatch::CommandSender;
use super::commands::{Command, CommandId};
use super::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShortcutModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub command: bool,
}

impl ShortcutModifiers {
    pub fn matches(&self, other: &egui::Modifiers) -> bool {
        self.ctrl == other.ctrl
            && self.alt == other.alt
            && self.shift == other.shift
            && self.command == other.command
    }

    pub fn to_egui(self) -> egui::Modifiers {
        let mut mods = egui::Modifiers::default();
        mods.ctrl = self.ctrl;
        mods.alt = self.alt;
        mods.shift = self.shift;
        mods.command = self.command;
        mods
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortcutBinding {
    pub key: Key,
    pub modifiers: ShortcutModifiers,
    primary: bool,
}

impl ShortcutBinding {
    pub fn new_primary(key: Key) -> Self {
        let mut modifiers = ShortcutModifiers::default();
        if cfg!(target_os = "macos") {
            modifiers.command = true;
        } else {
            modifiers.ctrl = true;
        }
        Self {
            key,
            modifiers,
            primary: true,
        }
    }

    pub fn with_shift(mut self) -> Self {
        self.modifiers.shift = true;
        self
    }

    pub fn with_alt(mut self) -> Self {
        self.modifiers.alt = true;
        self
    }

    pub fn with_ctrl(mut self) -> Self {
        self.modifiers.ctrl = true;
        self
    }

    pub fn with_command(mut self) -> Self {
        self.modifiers.command = true;
        self
    }

    pub fn new_fixed(key: Key) -> Self {
        Self {
            key,
            modifiers: ShortcutModifiers::default(),
            primary: false,
        }
    }

    pub fn new_fixed_with_modifiers(key: Key, modifiers: ShortcutModifiers) -> Self {
        Self {
            key,
            modifiers,
            primary: false,
        }
    }

    pub fn is_plain(&self) -> bool {
        !self.modifiers.alt
            && !self.modifiers.shift
            && !self.modifiers.ctrl
            && !self.modifiers.command
    }

    fn modifiers_for_platform(&self, is_macos: bool) -> ShortcutModifiers {
        let mut modifiers = self.modifiers;
        if self.primary {
            if is_macos {
                modifiers.command = true;
                modifiers.ctrl = false;
            } else {
                modifiers.command = false;
                modifiers.ctrl = true;
            }
        }
        modifiers
    }

    pub fn format_for_menu(&self) -> Option<String> {
        let mac = self.format_for_os(true)?;
        let other = self.format_for_os(false)?;
        if mac == other {
            Some(mac)
        } else {
            Some(format!("{mac} / {other}"))
        }
    }

    fn format_for_os(&self, is_macos: bool) -> Option<String> {
        let modifiers = self.modifiers_for_platform(is_macos);
        let mut parts: Vec<String> = Vec::new();
        if modifiers.command {
            if is_macos {
                parts.push("⌘".to_string());
            } else {
                parts.push("Ctrl".to_string());
            }
        }
        if modifiers.ctrl && !is_macos {
            parts.push("Ctrl".to_string());
        } else if modifiers.ctrl && is_macos && !self.primary {
            parts.push("Ctrl".to_string());
        }
        if modifiers.alt {
            if is_macos {
                parts.push("⌥".to_string());
            } else {
                parts.push("Alt".to_string());
            }
        }
        if modifiers.shift {
            if is_macos {
                parts.push("⇧".to_string());
            } else {
                parts.push("Shift".to_string());
            }
        }
        let key = key_to_string(self.key);
        if key.is_empty() {
            return None;
        }
        if is_macos {
            parts.push(key);
            Some(parts.join(""))
        } else {
            parts.push(key);
            Some(parts.join("+"))
        }
    }

    pub fn matches(&self, input: &egui::InputState) -> bool {
        input.key_pressed(self.key) && self.modifiers.matches(&input.modifiers)
    }

    pub fn parse(binding: &str) -> Option<Self> {
        let mut modifiers = ShortcutModifiers::default();
        let mut key = None;
        let mut primary = false;
        for token in binding.split('+') {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            match trimmed.to_lowercase().as_str() {
                "ctrl" => {
                    modifiers.ctrl = true;
                    primary = true;
                }
                "cmd" | "command" | "⌘" => {
                    modifiers.command = true;
                    primary = true;
                }
                "alt" | "option" | "⌥" => modifiers.alt = true,
                "shift" | "⇧" => modifiers.shift = true,
                other => {
                    key = parse_key(other);
                }
            }
        }
        let key = key?;
        Some(Self {
            key,
            modifiers,
            primary,
        })
    }
}

fn key_to_string(key: Key) -> String {
    match key {
        Key::A => "A".into(),
        Key::B => "B".into(),
        Key::C => "C".into(),
        Key::D => "D".into(),
        Key::E => "E".into(),
        Key::F => "F".into(),
        Key::G => "G".into(),
        Key::H => "H".into(),
        Key::I => "I".into(),
        Key::J => "J".into(),
        Key::K => "K".into(),
        Key::L => "L".into(),
        Key::M => "M".into(),
        Key::N => "N".into(),
        Key::O => "O".into(),
        Key::P => "P".into(),
        Key::Q => "Q".into(),
        Key::R => "R".into(),
        Key::S => "S".into(),
        Key::T => "T".into(),
        Key::U => "U".into(),
        Key::V => "V".into(),
        Key::W => "W".into(),
        Key::X => "X".into(),
        Key::Y => "Y".into(),
        Key::Z => "Z".into(),
        Key::Plus => "+".into(),
        Key::Minus => "-".into(),
        Key::Space => "Space".into(),
        Key::F11 => "F11".into(),
        Key::F12 => "F12".into(),
        Key::Delete => "Del".into(),
        Key::Home => "Home".into(),
        Key::Num0 => "0".into(),
        Key::Num1 => "1".into(),
        Key::Num2 => "2".into(),
        Key::Num3 => "3".into(),
        Key::Num4 => "4".into(),
        Key::Num5 => "5".into(),
        Key::Num6 => "6".into(),
        Key::Num7 => "7".into(),
        Key::Num8 => "8".into(),
        Key::Num9 => "9".into(),
        _ => format!("{key:?}"),
    }
}

fn parse_key(token: &str) -> Option<Key> {
    match token.to_uppercase().as_str() {
        "A" => Some(Key::A),
        "B" => Some(Key::B),
        "C" => Some(Key::C),
        "D" => Some(Key::D),
        "E" => Some(Key::E),
        "F" => Some(Key::F),
        "G" => Some(Key::G),
        "H" => Some(Key::H),
        "I" => Some(Key::I),
        "J" => Some(Key::J),
        "K" => Some(Key::K),
        "L" => Some(Key::L),
        "M" => Some(Key::M),
        "N" => Some(Key::N),
        "O" => Some(Key::O),
        "P" => Some(Key::P),
        "Q" => Some(Key::Q),
        "R" => Some(Key::R),
        "S" => Some(Key::S),
        "T" => Some(Key::T),
        "U" => Some(Key::U),
        "V" => Some(Key::V),
        "W" => Some(Key::W),
        "X" => Some(Key::X),
        "Y" => Some(Key::Y),
        "Z" => Some(Key::Z),
        "+" | "PLUS" => Some(Key::Plus),
        "-" | "MINUS" => Some(Key::Minus),
        "SPACE" => Some(Key::Space),
        "DEL" | "DELETE" => Some(Key::Delete),
        "HOME" => Some(Key::Home),
        "0" => Some(Key::Num0),
        "1" => Some(Key::Num1),
        "2" => Some(Key::Num2),
        "3" => Some(Key::Num3),
        "4" => Some(Key::Num4),
        "5" => Some(Key::Num5),
        "6" => Some(Key::Num6),
        "7" => Some(Key::Num7),
        "8" => Some(Key::Num8),
        "9" => Some(Key::Num9),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        _ => None,
    }
}

pub struct ShortcutMap {
    by_command: HashMap<CommandId, ShortcutBinding>,
    by_binding: HashMap<ShortcutBinding, Vec<CommandId>>,
}

impl ShortcutMap {
    pub fn load() -> Self {
        let mut map = Self {
            by_command: HashMap::new(),
            by_binding: HashMap::new(),
        };
        for (command, binding) in default_bindings() {
            map.insert(command, binding);
        }
        let file = config::load_shortcut_file();
        for entry in file.bindings {
            if let Some(binding) = ShortcutBinding::parse(&entry.binding) {
                map.insert(entry.command, binding);
            }
        }
        map
    }

    fn insert(&mut self, command: CommandId, binding: ShortcutBinding) {
        if let Some(old_binding) = self.by_command.insert(command, binding) {
            if let Some(commands) = self.by_binding.get_mut(&old_binding) {
                commands.retain(|entry| entry != &command);
                if commands.is_empty() {
                    self.by_binding.remove(&old_binding);
                }
            }
        }
        self.by_binding
            .entry(binding)
            .or_insert_with(Vec::new)
            .push(command);
    }

    pub fn binding_for(&self, id: CommandId) -> Option<&ShortcutBinding> {
        self.by_command.get(&id)
    }

    pub fn label_for(&self, id: CommandId) -> Option<String> {
        self.binding_for(id)
            .and_then(|binding| binding.format_for_menu())
    }

    pub fn handle_input(&self, ctx: &Context, sender: &CommandSender) {
        let wants_text = ctx.wants_keyboard_input();
        let mut triggered: Vec<(CommandId, ShortcutBinding)> = Vec::new();
        ctx.input(|input| {
            for (binding, commands) in &self.by_binding {
                if binding.is_plain() && wants_text {
                    continue;
                }
                if binding.matches(input) {
                    for command in commands {
                        triggered.push((*command, *binding));
                    }
                }
            }
        });
        for (command_id, binding) in triggered {
            ctx.input_mut(|state| {
                state.consume_key(binding.modifiers.to_egui(), binding.key);
            });
            if let Some(command) = Command::from_id(command_id) {
                let _ = sender.try_send(command);
            }
        }
    }
}

fn default_bindings() -> Vec<(CommandId, ShortcutBinding)> {
    use CommandId::*;
    vec![
        (FileNew, ShortcutBinding::new_primary(Key::N)),
        (FileOpen, ShortcutBinding::new_primary(Key::O)),
        (FileSave, ShortcutBinding::new_primary(Key::S)),
        (
            FileSaveAs,
            ShortcutBinding::new_primary(Key::S).with_shift(),
        ),
        (FileExport, ShortcutBinding::new_primary(Key::E)),
        (EditUndo, ShortcutBinding::new_primary(Key::Z)),
        (EditRedo, ShortcutBinding::new_primary(Key::Z).with_shift()),
        (EditCut, ShortcutBinding::new_primary(Key::X)),
        (EditCopy, ShortcutBinding::new_primary(Key::C)),
        (EditPaste, ShortcutBinding::new_primary(Key::V)),
        (EditDelete, ShortcutBinding::new_fixed(Key::Delete)),
        (EditSelectAll, ShortcutBinding::new_primary(Key::A)),
        (ViewToggleMixer, ShortcutBinding::new_fixed(Key::F9)),
        (ViewTogglePlaylist, ShortcutBinding::new_fixed(Key::F5)),
        (ViewToggleSequencer, ShortcutBinding::new_fixed(Key::F6)),
        (ViewTogglePianoRoll, ShortcutBinding::new_fixed(Key::P)),
        (ViewToggleBrowser, ShortcutBinding::new_fixed(Key::B)),
        (ViewZoomIn, ShortcutBinding::new_primary(Key::Plus)),
        (ViewZoomOut, ShortcutBinding::new_primary(Key::Minus)),
        (ViewToggleFullscreen, ShortcutBinding::new_fixed(Key::F11)),
        (ViewTogglePerfHud, ShortcutBinding::new_fixed(Key::F12)),
        (TrackArmRecord, ShortcutBinding::new_fixed(Key::R)),
        (TrackSolo, ShortcutBinding::new_fixed(Key::S)),
        (TrackMute, ShortcutBinding::new_fixed(Key::M)),
        (MidiQuantize, ShortcutBinding::new_fixed(Key::Q)),
        (TransportPlayPause, ShortcutBinding::new_fixed(Key::Space)),
        (TransportStop, ShortcutBinding::new_fixed(Key::Num0)),
        (TransportRecord, ShortcutBinding::new_primary(Key::R)),
        (TransportLoop, ShortcutBinding::new_fixed(Key::L)),
        (TransportGoToStart, ShortcutBinding::new_fixed(Key::Home)),
        (TransportTapTempo, ShortcutBinding::new_fixed(Key::T)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcut_binding() {
        let binding = ShortcutBinding::parse("Ctrl+Shift+S").unwrap();
        assert!(binding.modifiers.ctrl);
        assert!(binding.modifiers.shift);
        assert_eq!(binding.key, Key::S);
    }

    #[test]
    fn primary_binding_uses_platform_modifier() {
        let binding = ShortcutBinding::new_primary(Key::S);
        if cfg!(target_os = "macos") {
            assert!(binding.modifiers.command);
            assert!(!binding.modifiers.ctrl);
        } else {
            assert!(binding.modifiers.ctrl);
            assert!(!binding.modifiers.command);
        }
    }

    #[test]
    fn formats_menu_label_for_primary_shortcut() {
        let binding = ShortcutBinding::new_primary(Key::S);
        let label = binding.format_for_menu().expect("label");
        assert!(label.contains("Ctrl+S"));
        assert!(label.contains("⌘S"));
    }

    #[test]
    fn default_bindings_include_save_shortcut() {
        let bindings = default_bindings();
        let save = bindings
            .iter()
            .find(|(command, _)| *command == CommandId::FileSave)
            .map(|(_, binding)| binding)
            .expect("missing save binding");
        assert_eq!(save.key, Key::S);
        assert!(save.modifiers.ctrl || save.modifiers.command);
    }
}
