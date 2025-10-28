use std::fs;
use std::path::PathBuf;

use crate::device::MidiInputConfig;

/// Persisted MIDI settings stored on disk.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MidiSettings {
    /// Configured hardware inputs.
    pub inputs: Vec<MidiInputConfig>,
    /// Enable QWERTY fallback keyboard.
    pub qwerty_enabled: bool,
}

fn settings_path() -> Option<PathBuf> {
    let mut base = dirs::config_dir()?;
    base.push("HarmoniqStudio");
    if !base.exists() {
        let _ = fs::create_dir_all(&base);
    }
    base.push("midi.json");
    Some(base)
}

/// Load settings from disk. Returns defaults if loading fails.
pub fn load() -> MidiSettings {
    let Some(path) = settings_path() else {
        return MidiSettings::default();
    };
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => MidiSettings::default(),
    }
}

/// Save settings to disk.
pub fn save(settings: &MidiSettings) {
    let Some(path) = settings_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        if let Err(err) = fs::write(&path, json) {
            tracing::warn!(?err, "failed to write midi settings");
        }
    }
}

/// Synchronise the device manager configuration with the loaded settings.
pub fn apply_settings<B: crate::device::MidiBackend>(
    mgr: &mut crate::device::MidiDeviceManager<B>,
    settings: &MidiSettings,
) {
    mgr.set_config(settings.inputs.clone());
}

/// Update settings based on the current manager state.
pub fn capture_settings<B: crate::device::MidiBackend>(
    mgr: &crate::device::MidiDeviceManager<B>,
    settings: &mut MidiSettings,
) {
    settings.inputs = mgr.config().to_vec();
}
