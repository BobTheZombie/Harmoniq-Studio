use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

/// Metadata describing a single CLAP plugin discovered during a scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCacheEntry {
    pub path: PathBuf,
    pub name: String,
    pub version: Option<String>,
    pub last_modified: SystemTime,
}

impl PluginCacheEntry {
    pub fn new(
        path: PathBuf,
        name: String,
        version: Option<String>,
        last_modified: SystemTime,
    ) -> Self {
        Self {
            path,
            name,
            version,
            last_modified,
        }
    }
}

/// Handles discovery of CLAP plugins with a persistent cache to avoid redundant filesystem work.
#[derive(Debug, Clone)]
pub struct PluginScanner {
    cache_path: PathBuf,
    extension: String,
}

impl PluginScanner {
    pub fn new(cache_path: impl Into<PathBuf>) -> Self {
        Self {
            cache_path: cache_path.into(),
            extension: ".clap".to_owned(),
        }
    }

    /// Perform a scan of the provided directories, returning the cached entries if they are
    /// still valid, otherwise rescanning the filesystem and updating the cache.
    pub fn scan_directories(&self, roots: &[PathBuf]) -> Result<Vec<PluginCacheEntry>> {
        let mut cache = self.load_cache().unwrap_or_default();
        let mut result = Vec::new();

        for root in roots {
            for entry in WalkDir::new(root) {
                let entry = entry?;
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case(self.extension.trim_start_matches('.')))
                    .unwrap_or(false)
                {
                    let metadata = fs::metadata(entry.path())?;
                    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    let path = entry.path().to_path_buf();

                    if let Some(existing) = cache.get(entry.path()) {
                        if existing.last_modified >= modified {
                            result.push(existing.clone());
                            continue;
                        }
                    }

                    let descriptor = self.parse_descriptor(entry.path(), modified)?;
                    cache.insert(path.clone(), descriptor.clone());
                    result.push(descriptor);
                }
            }
        }

        self.save_cache(&cache)?;
        Ok(result)
    }

    fn parse_descriptor(&self, path: &Path, modified: SystemTime) -> Result<PluginCacheEntry> {
        let file_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Unknown");
        // Future work: Parse CLAP plugin metadata. For now we derive from file name.
        Ok(PluginCacheEntry::new(
            path.to_path_buf(),
            file_name.to_string(),
            None,
            modified,
        ))
    }

    fn load_cache(&self) -> Result<HashMap<PathBuf, PluginCacheEntry>> {
        if !self.cache_path.exists() {
            return Ok(HashMap::new());
        }
        let mut file = fs::File::open(&self.cache_path)
            .with_context(|| format!("Failed to open cache at {:?}", self.cache_path))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        if buf.is_empty() {
            return Ok(HashMap::new());
        }
        let raw: Vec<PluginCacheEntry> = serde_json::from_slice(&buf)
            .with_context(|| format!("Failed to deserialize cache {:?}", self.cache_path))?;
        Ok(raw
            .into_iter()
            .map(|entry| (entry.path.clone(), entry))
            .collect())
    }

    fn save_cache(&self, cache: &HashMap<PathBuf, PluginCacheEntry>) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut entries: Vec<_> = cache.values().cloned().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let data = serde_json::to_vec_pretty(&entries)?;
        fs::write(&self.cache_path, data)?;
        Ok(())
    }

    /// Remove the cache file from disk.
    pub fn invalidate(&self) -> io::Result<()> {
        if self.cache_path.exists() {
            fs::remove_file(&self.cache_path)
        } else {
            Ok(())
        }
    }
}
