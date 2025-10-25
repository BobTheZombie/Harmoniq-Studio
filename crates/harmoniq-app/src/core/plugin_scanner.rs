use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::plugin_registry::PluginRegistry;

/// Supported binary formats that the Harmoniq Studio plugin manager can scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginFormat {
    Clap,
    Vst3,
    Lv2,
}

impl PluginFormat {
    pub fn label(self) -> &'static str {
        match self {
            PluginFormat::Clap => "CLAP",
            PluginFormat::Vst3 => "VST3",
            PluginFormat::Lv2 => "LV2",
        }
    }
}

/// Metadata describing a discoverable plugin binary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginDescriptor {
    pub name: String,
    pub format: PluginFormat,
    pub path: PathBuf,
    pub vendor: Option<String>,
    pub version: Option<String>,
}

impl PluginDescriptor {
    fn from_path(path: PathBuf, format: PluginFormat) -> Self {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Unknown Plugin")
            .to_string();
        Self {
            name,
            format,
            vendor: infer_vendor(&path),
            version: None,
            path,
        }
    }
}

/// Snapshot of the background scan progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStatus {
    pub message: String,
    pub completed: usize,
    pub total: usize,
}

/// State machine for the asynchronous scanning job.
#[derive(Debug, Clone)]
pub enum ScanState {
    Idle,
    Scanning(ScanStatus),
    Completed {
        plugins: Vec<PluginDescriptor>,
        errors: Vec<String>,
    },
    Failed(String),
}

impl Default for ScanState {
    fn default() -> Self {
        ScanState::Idle
    }
}

/// Handle returned by [`PluginScanner::scan_async`] for polling progress.
#[derive(Clone)]
pub struct ScanHandle {
    state: Arc<Mutex<ScanState>>,
}

impl ScanHandle {
    pub fn snapshot(&self) -> ScanState {
        self.state.lock().clone()
    }
}

/// Background scanner that walks the filesystem on a worker thread.
#[derive(Clone)]
pub struct PluginScanner {
    registry: Arc<PluginRegistry>,
}

impl PluginScanner {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self { registry }
    }

    pub fn cached_plugins(&self) -> Vec<PluginDescriptor> {
        self.registry.plugins()
    }

    pub fn scan_async(&self) -> ScanHandle {
        let state = Arc::new(Mutex::new(ScanState::Scanning(ScanStatus {
            message: "Locating plugin folders…".to_string(),
            completed: 0,
            total: 1,
        })));
        let registry = Arc::clone(&self.registry);
        let state_for_thread = Arc::clone(&state);
        thread::spawn(move || match run_scan(state_for_thread.clone()) {
            Ok((plugins, errors)) => {
                if let Err(err) = registry.set_plugins(plugins.clone()) {
                    *state_for_thread.lock() = ScanState::Failed(err.to_string());
                } else {
                    *state_for_thread.lock() = ScanState::Completed { plugins, errors };
                }
            }
            Err(err) => {
                *state_for_thread.lock() = ScanState::Failed(err.to_string());
            }
        });
        ScanHandle { state }
    }
}

fn run_scan(state: Arc<Mutex<ScanState>>) -> Result<(Vec<PluginDescriptor>, Vec<String>)> {
    let candidates = candidate_directories();
    let total = candidates.len();
    if total == 0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();

    update_status(&state, "Starting plugin scan…", 0, total);

    for (index, (dir, format)) in candidates.into_iter().enumerate() {
        update_status(&state, format!("Scanning {}", dir.display()), index, total);
        match scan_directory(&dir, format, &mut results, &mut seen, &state) {
            Ok(dir_errors) => {
                errors.extend(dir_errors);
            }
            Err(err) => errors.push(err.to_string()),
        }
        update_status(
            &state,
            format!("Finished {}", dir.display()),
            index + 1,
            total,
        );
    }

    results.sort_by(|a, b| {
        let name_cmp = a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase());
        if name_cmp == std::cmp::Ordering::Equal {
            a.format.label().cmp(b.format.label())
        } else {
            name_cmp
        }
    });

    Ok((results, errors))
}

fn scan_directory(
    dir: &Path,
    format: PluginFormat,
    results: &mut Vec<PluginDescriptor>,
    seen: &mut HashSet<PathBuf>,
    state: &Arc<Mutex<ScanState>>,
) -> Result<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut local_errors = Vec::new();
    let read_dir =
        fs::read_dir(dir).with_context(|| format!("unable to read {}", dir.display()))?;
    let mut entries: VecDeque<_> = read_dir.collect();
    let total_entries = entries.len().max(1);
    for (idx, entry) in entries.drain(..).enumerate() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                local_errors.push(format!("{}: {}", dir.display(), err));
                continue;
            }
        };
        let path = entry.path();
        update_status(
            state,
            format!(
                "Inspecting {}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("plugin")
            ),
            idx,
            total_entries,
        );
        if !matches_plugin(&path, format) {
            continue;
        }
        if seen.insert(path.clone()) {
            results.push(PluginDescriptor::from_path(path, format));
        }
    }
    Ok(local_errors)
}

fn matches_plugin(path: &Path, format: PluginFormat) -> bool {
    match format {
        PluginFormat::Clap => path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("clap"))
            .unwrap_or(false),
        PluginFormat::Vst3 => path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".vst3"))
            .unwrap_or(false),
        PluginFormat::Lv2 => {
            if path.is_dir() {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("lv2"))
                    .unwrap_or(false)
            } else {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("so"))
                    .unwrap_or(false)
            }
        }
    }
}

fn candidate_directories() -> Vec<(PathBuf, PluginFormat)> {
    let mut candidates = Vec::new();
    for format in [PluginFormat::Clap, PluginFormat::Vst3, PluginFormat::Lv2] {
        candidates.extend(default_roots(format).into_iter().map(|path| (path, format)));
    }
    candidates
        .into_iter()
        .filter(|(path, _)| path.exists())
        .collect()
}

fn default_roots(format: PluginFormat) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    match format {
        PluginFormat::Clap => {
            roots.push(PathBuf::from("/usr/lib/clap"));
            roots.push(PathBuf::from("/usr/local/lib/clap"));
            if let Some(home) = dirs::home_dir() {
                roots.push(home.join(".clap"));
            }
            #[cfg(target_os = "macos")]
            {
                roots.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
                if let Some(home) = dirs::home_dir() {
                    roots.push(home.join("Library/Audio/Plug-Ins/CLAP"));
                }
            }
        }
        PluginFormat::Vst3 => {
            roots.push(PathBuf::from("/usr/lib/vst3"));
            roots.push(PathBuf::from("/usr/local/lib/vst3"));
            if let Some(home) = dirs::home_dir() {
                roots.push(home.join(".vst3"));
            }
            #[cfg(target_os = "macos")]
            {
                roots.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
                if let Some(home) = dirs::home_dir() {
                    roots.push(home.join("Library/Audio/Plug-Ins/VST3"));
                }
            }
            #[cfg(target_os = "windows")]
            {
                if let Some(program_files) = std::env::var_os("PROGRAMFILES").map(PathBuf::from) {
                    roots.push(program_files.join("Common Files/VST3"));
                }
            }
        }
        PluginFormat::Lv2 => {
            roots.push(PathBuf::from("/usr/lib/lv2"));
            roots.push(PathBuf::from("/usr/local/lib/lv2"));
            if let Some(home) = dirs::home_dir() {
                roots.push(home.join(".lv2"));
            }
            #[cfg(target_os = "macos")]
            {
                roots.push(PathBuf::from("/Library/Audio/Plug-Ins/LV2"));
                if let Some(home) = dirs::home_dir() {
                    roots.push(home.join("Library/Audio/Plug-Ins/LV2"));
                }
            }
            #[cfg(target_os = "windows")]
            {
                if let Some(program_files) = std::env::var_os("PROGRAMFILES").map(PathBuf::from) {
                    roots.push(program_files.join("LV2"));
                }
            }
        }
    }
    roots
}

fn infer_vendor(path: &Path) -> Option<String> {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

fn update_status(
    state: &Arc<Mutex<ScanState>>,
    message: impl Into<String>,
    completed: usize,
    total: usize,
) {
    *state.lock() = ScanState::Scanning(ScanStatus {
        message: message.into(),
        completed,
        total,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn detect_plugins_in_directory() {
        let dir = tempdir().unwrap();
        let clap_dir = dir.path().join("test.clap");
        fs::write(&clap_dir, b"").unwrap();
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        let state = Arc::new(Mutex::new(ScanState::Idle));
        scan_directory(
            dir.path(),
            PluginFormat::Clap,
            &mut results,
            &mut seen,
            &state,
        )
        .unwrap();
        assert_eq!(results.len(), 1);
    }
}
