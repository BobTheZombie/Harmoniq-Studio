use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use directories::BaseDirs;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::plugin_scanner::PluginDescriptor;

/// Persistent cache of discovered plugin metadata.
///
/// The registry keeps a copy of the last successful scan on disk so that the
/// DAW can populate the user interface immediately on startup without hitting
/// the filesystem or probing binaries on the UI thread. The cache is stored in
/// the user's configuration directory (`$HOME/.config/mydaw/plugins_cache.json`).
#[derive(Debug)]
pub struct PluginRegistry {
    path: PathBuf,
    plugins: RwLock<Vec<PluginDescriptor>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    plugins: Vec<PluginDescriptor>,
}

impl PluginRegistry {
    /// Loads the registry from the default cache location.
    pub fn load_default() -> Result<Self> {
        let path = default_cache_path()?;
        Self::load_from_path(path)
    }

    /// Loads the registry from an explicit path on disk.
    pub fn load_from_path(path: PathBuf) -> Result<Self> {
        let plugins = read_registry_file(&path)?;
        Ok(Self {
            path,
            plugins: RwLock::new(plugins),
        })
    }

    /// Returns the cached plugin descriptors.
    pub fn plugins(&self) -> Vec<PluginDescriptor> {
        self.plugins.read().clone()
    }

    /// Overwrites the cached plugin descriptors and persists them to disk.
    pub fn set_plugins(&self, plugins: Vec<PluginDescriptor>) -> Result<()> {
        {
            let mut guard = self.plugins.write();
            *guard = plugins.clone();
        }
        persist_registry_file(&self.path, &plugins)
    }

    /// Returns a shareable handle that can be cloned across threads.
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

fn default_cache_path() -> Result<PathBuf> {
    let dirs = BaseDirs::new().context("unable to determine home directory")?;
    let mut path = dirs.config_dir().to_path_buf();
    path.push("mydaw");
    fs::create_dir_all(&path).with_context(|| {
        format!(
            "failed to create plugin cache directory at {}",
            path.display()
        )
    })?;
    path.push("plugins_cache.json");
    Ok(path)
}

fn read_registry_file(path: &Path) -> Result<Vec<PluginDescriptor>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read plugin cache at {}", path.display()))?;
    let file: RegistryFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse plugin cache at {}", path.display()))?;
    Ok(file.plugins)
}

fn persist_registry_file(path: &Path, plugins: &[PluginDescriptor]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create plugin cache directory at {}",
                parent.display()
            )
        })?;
    }
    let data = RegistryFile {
        plugins: plugins.to_vec(),
    };
    let json = serde_json::to_string_pretty(&data).context("failed to serialize plugin cache")?;
    fs::write(path, json)
        .with_context(|| format!("failed to write plugin cache at {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    use crate::core::plugin_scanner::{PluginDescriptor, PluginFormat};

    #[test]
    fn writes_and_reads_cache() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let registry = PluginRegistry::load_from_path(path.clone()).unwrap();
        let mut descriptor = PluginDescriptor {
            name: "Test".to_string(),
            format: PluginFormat::Clap,
            path: path.clone(),
            vendor: Some("Vendor".into()),
            version: Some("1.0.0".into()),
        };
        registry.set_plugins(vec![descriptor.clone()]).unwrap();
        let loaded = registry.plugins();
        assert_eq!(loaded.len(), 1);
        descriptor.path = path;
        assert_eq!(loaded[0].name, descriptor.name);
    }
}
