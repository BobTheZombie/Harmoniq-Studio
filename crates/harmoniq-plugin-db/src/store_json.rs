use std::fs;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::entry::PluginEntry;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("failed to read plugin database: {0}")]
    Read(#[from] std::io::Error),
    #[error("failed to parse plugin database: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct JsonStoreData {
    plugins: Vec<PluginEntry>,
}

#[derive(Debug)]
pub struct PluginStore {
    path: PathBuf,
    data: Mutex<JsonStoreData>,
}

impl PluginStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let data = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            JsonStoreData::default()
        };
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    pub fn default_path() -> Result<PathBuf, StoreError> {
        let mut config_dir = dirs::config_dir().ok_or_else(|| {
            StoreError::Read(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no config directory",
            ))
        })?;
        config_dir.push("HarmoniqStudio");
        fs::create_dir_all(&config_dir)?;
        config_dir.push("plugins.json");
        Ok(config_dir)
    }

    pub fn upsert(&self, entry: PluginEntry) -> Result<(), StoreError> {
        let mut data = self.data.lock();
        if let Some(existing) = data.plugins.iter_mut().find(|plugin| plugin.id == entry.id) {
            *existing = entry;
        } else {
            data.plugins.push(entry);
        }
        self.persist_locked(&data)
    }

    pub fn plugins(&self) -> Vec<PluginEntry> {
        self.data.lock().plugins.clone()
    }

    fn persist_locked(&self, data: &JsonStoreData) -> Result<(), StoreError> {
        let json = serde_json::to_string_pretty(data)?;
        fs::write(&self.path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::*;
    use crate::entry::{PluginFormat, PluginId};

    #[test]
    fn upsert_adds_and_updates() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plugins.json");
        let store = PluginStore::open(&path).unwrap();
        let mut entry = PluginEntry::new(PluginId::new(PluginFormat::Clap, "a"), "A");
        entry.verified = true;
        store.upsert(entry.clone()).unwrap();
        let mut updated = entry.clone();
        updated.name = "Updated".into();
        store.upsert(updated.clone()).unwrap();
        let plugins = store.plugins();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "Updated");
    }
}
