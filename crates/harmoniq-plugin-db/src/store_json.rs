use std::fs;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::entry::PluginEntry;
use crate::stock::stock_instruments;

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
        let mut data = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            JsonStoreData::default()
        };
        let seeded = seed_stock_instruments(&mut data.plugins);
        let store = Self {
            path,
            data: Mutex::new(data),
        };
        if seeded {
            let guard = store.data.lock();
            store.persist_locked(&guard)?;
        }
        Ok(store)
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

fn seed_stock_instruments(plugins: &mut Vec<PluginEntry>) -> bool {
    let mut modified = false;
    for stock in stock_instruments() {
        if let Some(existing) = plugins
            .iter_mut()
            .find(|plugin| plugin.reference == stock.reference)
        {
            let mut changed = false;
            if existing.name != stock.name {
                existing.name = stock.name.clone();
                changed = true;
            }
            if existing.vendor != stock.vendor {
                existing.vendor = stock.vendor.clone();
                changed = true;
            }
            if existing.category != stock.category {
                existing.category = stock.category.clone();
                changed = true;
            }
            if existing.version != stock.version {
                existing.version = stock.version.clone();
                changed = true;
            }
            if existing.description != stock.description {
                existing.description = stock.description.clone();
                changed = true;
            }
            if existing.is_instrument != stock.is_instrument {
                existing.is_instrument = stock.is_instrument;
                changed = true;
            }
            if existing.has_editor != stock.has_editor {
                existing.has_editor = stock.has_editor;
                changed = true;
            }
            if existing.num_inputs != stock.num_inputs {
                existing.num_inputs = stock.num_inputs;
                changed = true;
            }
            if existing.num_outputs != stock.num_outputs {
                existing.num_outputs = stock.num_outputs;
                changed = true;
            }
            if existing.quarantined {
                existing.quarantined = false;
                changed = true;
            }
            if stock.last_seen > existing.last_seen {
                existing.last_seen = stock.last_seen;
                changed = true;
            }
            if changed {
                modified = true;
            }
        } else {
            plugins.push(stock);
            modified = true;
        }
    }
    modified
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
        let initial_len = store.plugins().len();
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
        let matches: Vec<_> = plugins
            .into_iter()
            .filter(|plugin| plugin.reference == updated.reference)
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Updated");
        assert!(store.plugins().len() >= initial_len + 1);
    }

    #[test]
    fn merge_deduplicates_and_sorts() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plugins.json");
        let store = PluginStore::open(&path).unwrap();
        let initial_len = store.plugins().len();
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
        assert!(plugins.len() >= initial_len + 2);
        let mut filtered: Vec<_> = plugins
            .into_iter()
            .filter(|plugin| {
                plugin.reference == newer.reference || plugin.reference == second.reference
            })
            .collect();
        filtered.sort_by(|a, b| a.reference.id.cmp(&b.reference.id));
        assert_eq!(filtered.len(), 2);
        let newer_entry = filtered
            .iter()
            .find(|entry| entry.reference == newer.reference)
            .unwrap();
        assert_eq!(newer_entry.name, "Newer");
        let second_entry = filtered
            .iter()
            .find(|entry| entry.reference == second.reference)
            .unwrap();
        assert_eq!(second_entry.reference, second.reference);
    }

    #[test]
    fn stock_instruments_seeded_on_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plugins.json");
        let store = PluginStore::open(&path).unwrap();
        let plugins = store.plugins();
        assert!(plugins
            .iter()
            .any(|plugin| plugin.reference.id == "harmoniq.analog"));
        assert!(plugins
            .iter()
            .any(|plugin| plugin.reference.id == "harmoniq.grand_piano_clap"));
    }
}
