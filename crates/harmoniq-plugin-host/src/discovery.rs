use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use dirs::home_dir;

use crate::error::HostError;

/// Supported binary formats for external plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginFormat {
    Vst3,
    Lv2,
    Clap,
    Harmoniq,
}

impl PluginFormat {
    pub fn label(self) -> &'static str {
        match self {
            PluginFormat::Vst3 => "VST3",
            PluginFormat::Lv2 => "LV2",
            PluginFormat::Clap => "CLAP",
            PluginFormat::Harmoniq => "Harmoniq",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginCategory {
    Instrument,
    Effect,
    Utility,
}

impl PluginCategory {
    fn infer_from_name(name: &str) -> Self {
        let lowercase = name.to_ascii_lowercase();
        if lowercase.contains("synth")
            || lowercase.contains("drum")
            || lowercase.contains("bass")
            || lowercase.contains("piano")
        {
            PluginCategory::Instrument
        } else if lowercase.contains("eq")
            || lowercase.contains("compress")
            || lowercase.contains("reverb")
            || lowercase.contains("delay")
            || lowercase.contains("filter")
        {
            PluginCategory::Effect
        } else {
            PluginCategory::Utility
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub path: PathBuf,
    pub format: PluginFormat,
    pub vendor: Option<String>,
    pub category: PluginCategory,
    pub has_native_editor: bool,
    pub tags: Vec<String>,
}

impl DiscoveredPlugin {
    pub fn new(path: PathBuf, format: PluginFormat) -> Self {
        let name = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("Unknown Plugin")
            .to_string();
        let category = PluginCategory::infer_from_name(&name);
        Self {
            name,
            path,
            format,
            vendor: None,
            category,
            has_native_editor: true,
            tags: Vec::new(),
        }
    }

    pub fn display_format(&self) -> &'static str {
        self.format.label()
    }
}

#[derive(Debug)]
pub struct DiscoveryResult {
    pub plugins: Vec<DiscoveredPlugin>,
    pub errors: Vec<HostError>,
}

impl DiscoveryResult {
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
            errors: Vec::new(),
        }
    }
}

pub fn discover_plugins() -> DiscoveryResult {
    let mut result = DiscoveryResult::empty();
    let mut candidates = Vec::new();

    if let Some(home) = home_dir() {
        candidates.push((home.join(".vst3"), PluginFormat::Vst3));
        candidates.push((home.join(".lv2"), PluginFormat::Lv2));
        candidates.push((home.join(".clap"), PluginFormat::Clap));
    }

    candidates.push((PathBuf::from("/usr/lib/vst3"), PluginFormat::Vst3));
    candidates.push((PathBuf::from("/usr/lib/lv2"), PluginFormat::Lv2));
    candidates.push((PathBuf::from("/usr/lib/clap"), PluginFormat::Clap));
    candidates.push((PathBuf::from("resources/plugins"), PluginFormat::Harmoniq));

    for (path, format) in candidates {
        if let Err(err) = scan_directory(&path, format, &mut result.plugins) {
            result.errors.push(err);
        }
    }

    result.plugins.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    result
}

fn scan_directory(
    path: &Path,
    format: PluginFormat,
    plugins: &mut Vec<DiscoveredPlugin>,
) -> Result<(), HostError> {
    if !path.exists() {
        return Ok(());
    }

    let read_dir = match fs::read_dir(path) {
        Ok(dir) => dir,
        Err(err) => return Err(HostError::Io(err)),
    };

    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        if matches_plugin(&entry_path, format) {
            plugins.push(DiscoveredPlugin::new(entry_path, format));
        }
    }

    Ok(())
}

fn matches_plugin(path: &Path, format: PluginFormat) -> bool {
    match format {
        PluginFormat::Vst3 => {
            path.extension().is_none()
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .map(|name| name.ends_with(".vst3"))
                    .unwrap_or(false)
        }
        PluginFormat::Lv2 => path
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| ext.eq_ignore_ascii_case("lv2"))
            .unwrap_or(false),
        PluginFormat::Clap => path
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| ext.eq_ignore_ascii_case("clap"))
            .unwrap_or(false),
        PluginFormat::Harmoniq => path
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| ext.eq_ignore_ascii_case("harmoniq"))
            .unwrap_or_else(|| path.is_file()),
    }
}
