use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use walkdir::WalkDir;

use crate::probe::probe_plugin;

#[derive(Debug, Serialize)]
pub struct ScannedPlugin {
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub vendor: String,
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub plugins: Vec<ScannedPlugin>,
}

pub fn scan_directories(user_paths: Vec<String>) -> Result<ScanResult> {
    let mut search_paths: Vec<PathBuf> = vec![
        dirs::home_dir().map(|home| home.join(".clap")),
        Some(PathBuf::from("/usr/lib/clap")),
        Some(PathBuf::from("/usr/local/lib/clap")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in user_paths {
        search_paths.push(PathBuf::from(path));
    }

    let mut plugins = Vec::new();
    let mut seen = BTreeMap::new();

    for path in search_paths {
        if !path.exists() {
            continue;
        }
        for entry in WalkDir::new(&path)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("clap") {
                continue;
            }
            if let Some(meta) = probe_plugin(entry.path())? {
                if seen.insert(meta.id.clone(), ()).is_none() {
                    plugins.push(meta);
                }
            }
        }
    }

    Ok(ScanResult { plugins })
}
