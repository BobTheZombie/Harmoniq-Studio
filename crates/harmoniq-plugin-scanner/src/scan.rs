use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

use harmoniq_plugin_db::{PluginEntry, PluginFormat, PluginId, PluginStore};

use crate::probe::ProbeResult;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub formats: Vec<PluginFormat>,
    pub extra_paths: Vec<PathBuf>,
    pub verify: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            formats: vec![PluginFormat::Clap, PluginFormat::Harmoniq],
            extra_paths: Vec::new(),
            verify: true,
        }
    }
}

pub struct Scanner {
    store: PluginStore,
}

impl Scanner {
    pub fn new(store: PluginStore) -> Self {
        Self { store }
    }

    pub fn scan(&self, options: &ScanOptions) -> Result<Vec<PluginEntry>> {
        let mut discovered = Vec::new();
        for format in &options.formats {
            for path in self.search_paths(*format, &options.extra_paths) {
                if let Ok(meta) = ProbeResult::simulate(path.clone(), *format) {
                    let id = PluginId::new(*format, path.to_string_lossy());
                    let mut entry = PluginEntry::new(id, meta.name);
                    entry.vendor = meta.vendor;
                    entry.category = meta.category;
                    entry.is_instrument = meta.is_instrument;
                    entry.supports_editor = meta.supports_editor;
                    entry.verified = options.verify;
                    self.store.upsert(entry.clone())?;
                    discovered.push(entry);
                }
            }
        }
        Ok(discovered)
    }

    fn search_paths(&self, format: PluginFormat, extra: &[PathBuf]) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for base in self.default_locations(format) {
            paths.extend(self.walk_plugins(base));
        }
        paths.extend(extra.to_vec());
        paths
    }

    fn default_locations(&self, format: PluginFormat) -> Vec<PathBuf> {
        match format {
            PluginFormat::Clap => vec![
                PathBuf::from("/usr/lib/clap"),
                dirs::home_dir()
                    .map(|mut home| {
                        home.push(".clap");
                        home
                    })
                    .unwrap_or_default(),
            ],
            PluginFormat::Vst3 => vec![PathBuf::from("/usr/lib/vst3")],
            PluginFormat::Harmoniq => vec![dirs::home_dir()
                .map(|mut home| {
                    home.push(".harmoniq/plugins");
                    home
                })
                .unwrap_or_default()],
        }
    }

    fn walk_plugins(&self, base: PathBuf) -> Vec<PathBuf> {
        let mut results = Vec::new();
        if !base.exists() {
            return results;
        }
        for entry in WalkDir::new(base).max_depth(2) {
            if let Ok(entry) = entry {
                let path = entry.path();
                if is_plugin_file(path) {
                    results.push(path.to_path_buf());
                }
            }
        }
        results
    }
}

fn is_plugin_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext, "clap" | "vst3" | "so" | "dll"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use harmoniq_plugin_db::PluginFormat;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn simulated_scan_discovers_files() {
        let dir = tempdir().unwrap();
        let fake_plugin = dir.path().join("test.clap");
        std::fs::write(&fake_plugin, "").unwrap();
        let store = PluginStore::open(dir.path().join("db.json")).unwrap();
        let scanner = Scanner::new(store);
        let mut options = ScanOptions::default();
        options.extra_paths.push(fake_plugin.clone());
        let result = scanner.scan(&options).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id.path, fake_plugin.to_string_lossy());
    }
}
