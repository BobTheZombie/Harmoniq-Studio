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
        if let Some(existing) = data
            .plugins
            .iter_mut()
            .find(|plugin| plugin.reference == entry.reference)
        {
            *existing = entry;
        } else {
            data.plugins.push(entry);
        }
        self.persist_locked(&data)
    }

    pub fn merge(&self, entries: Vec<PluginEntry>) -> Result<(), StoreError> {
        let mut data = self.data.lock();
        for entry in entries {
            if let Some(existing) = data
                .plugins
                .iter_mut()
                .find(|plugin| plugin.reference == entry.reference)
            {
                if entry.last_seen > existing.last_seen {
                    *existing = entry;
                }
            } else {
                data.plugins.push(entry);
            }
        }
        data.plugins.sort_by(|a, b| {
            b.last_seen
                .cmp(&a.last_seen)
                .then_with(|| a.name.cmp(&b.name))
        });
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
    use crate::entry::{PluginFormat, PluginMetadata};

    #[test]
    fn upsert_adds_and_updates() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plugins.json");
        let store = PluginStore::open(&path).unwrap();
        let entry = PluginMetadata {
            id: "a".into(),
            name: "A".into(),
            vendor: None,
            category: None,
            version: None,
            description: None,
            is_instrument: false,
            has_editor: false,
            num_inputs: 0,
            num_outputs: 2,
        }
        .into_entry(PluginFormat::Clap, "/tmp/a");
        store.upsert(entry.clone()).unwrap();
        let mut updated = entry.clone();
        updated.name = "Updated".into();
        store.upsert(updated.clone()).unwrap();
        let plugins = store.plugins();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "Updated");
    }

    #[test]
    fn merge_deduplicates_and_sorts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plugins.json");
        let store = PluginStore::open(&path).unwrap();
        let mut first = PluginMetadata {
            id: "a".into(),
            name: "A".into(),
            vendor: None,
            category: None,
            version: None,
            description: None,
            is_instrument: false,
            has_editor: false,
            num_inputs: 0,
            num_outputs: 2,
        }
        .into_entry(PluginFormat::Clap, "/tmp/a");
        first.last_seen = Utc::now();
        let mut second = PluginMetadata {
            id: "b".into(),
            name: "B".into(),
            vendor: None,
            category: None,
            version: None,
            description: None,
            is_instrument: false,
            has_editor: false,
            num_inputs: 0,
            num_outputs: 2,
        }
        .into_entry(PluginFormat::Vst3, "/tmp/b");
        second.last_seen = Utc::now();
        store.merge(vec![first.clone(), second.clone()]).unwrap();
        let mut newer = first.clone();
        newer.name = "Newer".into();
        newer.last_seen = Utc::now() + chrono::Duration::seconds(10);
        store.merge(vec![newer.clone()]).unwrap();
        let plugins = store.plugins();
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].name, "Newer");
        assert_eq!(plugins[1].reference, second.reference);
    }
}
