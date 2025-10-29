use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use harmoniq_plugin_db::{
    scan_plugins, ManifestProber, PluginEntry, PluginFormat, PluginStore, ScanConfig,
};

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub formats: Vec<PluginFormat>,
    pub extra_paths: Vec<PathBuf>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            formats: vec![
                PluginFormat::Clap,
                PluginFormat::Vst3,
                PluginFormat::Ovst3,
                PluginFormat::Harmoniq,
            ],
            extra_paths: Vec::new(),
        }
    }
}

pub struct Scanner {
    store: Arc<PluginStore>,
}

impl Scanner {
    pub fn new(store: Arc<PluginStore>) -> Self {
        Self { store }
    }

    pub fn scan(&self, options: &ScanOptions) -> Result<Vec<PluginEntry>> {
        let mut config = ScanConfig::default();
        let mut user_roots: HashSet<PathBuf> = config.user_roots.into_iter().collect();
        for extra in &options.extra_paths {
            if extra.is_dir() {
                user_roots.insert(extra.clone());
            } else if let Some(parent) = extra.parent() {
                user_roots.insert(parent.to_path_buf());
            }
        }
        config.user_roots = user_roots.into_iter().collect();
        let report = scan_plugins(&config, &ManifestProber::default());
        let mut entries: Vec<_> = report
            .entries
            .into_iter()
            .filter(|entry| options.formats.contains(&entry.reference.format))
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        self.store.merge(entries.clone())?;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use harmoniq_plugin_db::{PluginFormat, PluginStore};
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn simulated_scan_discovers_files() {
        let dir = tempdir().unwrap();
        let clap_root = dir.path().join("clap");
        std::fs::create_dir_all(&clap_root).unwrap();
        let fake_plugin = clap_root.join("test.clap");
        std::fs::write(&fake_plugin, "").unwrap();
        let store = Arc::new(PluginStore::open(dir.path().join("db.json")).unwrap());
        let scanner = Scanner::new(Arc::clone(&store));
        let mut options = ScanOptions::default();
        options.formats = vec![PluginFormat::Clap];
        options.extra_paths.push(clap_root.clone());
        let result = scanner.scan(&options).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].reference.path, fake_plugin.to_string_lossy());
    }
}
