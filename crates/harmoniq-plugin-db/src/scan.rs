use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Deserialize;
use walkdir::WalkDir;

use crate::{PluginEntry, PluginFormat, PluginMetadata, PluginRef};

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub system_roots: Vec<PathBuf>,
    pub user_roots: Vec<PathBuf>,
    pub max_depth: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        let system_roots = vec![
            PathBuf::from("/usr/share/harmoniq-studio/plugins/clap"),
            PathBuf::from("/usr/share/harmoniq-studio/plugins/vst3"),
            PathBuf::from("/usr/share/harmoniq-studio/plugins/ovst3"),
            PathBuf::from("/usr/share/harmoniq-studio/plugins/harmoniq"),
        ];
        let mut user_roots = Vec::new();
        if let Some(home) = dirs::home_dir() {
            user_roots.push(home.join(".clap"));
            user_roots.push(home.join(".vst3"));
            user_roots.push(home.join(".harmoniq/plugins"));
        }
        Self {
            system_roots,
            user_roots,
            max_depth: 4,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid manifest: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("unsupported plugin format")]
    Unsupported,
}

pub trait PluginProber {
    fn probe(&self, format: PluginFormat, path: &Path) -> Result<PluginMetadata, ProbeError>;
}

#[derive(Debug, Default)]
pub struct ManifestProber;

#[derive(Debug, Deserialize)]
struct ManifestFile {
    pub id: Option<String>,
    pub name: Option<String>,
    pub vendor: Option<String>,
    pub category: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub is_instrument: Option<bool>,
    pub has_editor: Option<bool>,
    pub num_inputs: Option<u32>,
    pub num_outputs: Option<u32>,
}

impl PluginProber for ManifestProber {
    fn probe(&self, format: PluginFormat, path: &Path) -> Result<PluginMetadata, ProbeError> {
        let manifest = find_manifest(format, path)?;
        if let Some(manifest) = manifest {
            let id = manifest
                .id
                .or_else(|| default_id(path))
                .unwrap_or_else(|| path.display().to_string());
            let name = manifest.name.unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            });
            Ok(PluginMetadata {
                id,
                name,
                vendor: manifest.vendor,
                category: manifest.category,
                version: manifest.version,
                description: manifest.description,
                is_instrument: manifest.is_instrument.unwrap_or(false),
                has_editor: manifest.has_editor.unwrap_or(false),
                num_inputs: manifest.num_inputs.unwrap_or(0),
                num_outputs: manifest.num_outputs.unwrap_or(2),
            })
        } else {
            Ok(stub_metadata(format, path))
        }
    }
}

fn find_manifest(
    format: PluginFormat,
    path: &Path,
) -> Result<Option<ManifestFile>, std::io::Error> {
    let mut candidates = Vec::new();
    match format {
        PluginFormat::Clap => {
            if path.is_dir() {
                candidates.push(path.join("manifest.json"));
                candidates.push(path.join("Contents/manifest.json"));
            } else if let Some(parent) = path.parent() {
                candidates.push(parent.join("manifest.json"));
            }
        }
        PluginFormat::Vst3 | PluginFormat::Ovst3 => {
            candidates.push(path.join("Contents/manifest.json"));
            candidates.push(path.join("manifest.json"));
        }
        PluginFormat::Harmoniq => {
            if path.is_file() {
                if let Some(parent) = path.parent() {
                    candidates.push(parent.join("manifest.json"));
                }
            }
            candidates.push(path.join("manifest.json"));
        }
    }
    for candidate in candidates {
        if candidate.exists() {
            let raw = fs::read_to_string(candidate)?;
            let manifest: ManifestFile = serde_json::from_str(&raw)?;
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

fn default_id(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

fn stub_metadata(format: PluginFormat, path: &Path) -> PluginMetadata {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let mut id = name.clone();
    id.push('#');
    id.push_str(match format {
        PluginFormat::Clap => "clap",
        PluginFormat::Vst3 => "vst3",
        PluginFormat::Ovst3 => "ovst3",
        PluginFormat::Harmoniq => "hq",
    });
    PluginMetadata {
        id,
        name,
        vendor: None,
        category: None,
        version: None,
        description: None,
        is_instrument: matches!(format, PluginFormat::Harmoniq),
        has_editor: false,
        num_inputs: 0,
        num_outputs: 2,
    }
}

#[derive(Debug)]
pub struct ScanReport {
    pub entries: Vec<PluginEntry>,
}

impl ScanReport {
    pub fn into_entries(self) -> Vec<PluginEntry> {
        self.entries
    }
}

pub fn scan_plugins<P: PluginProber>(config: &ScanConfig, prober: &P) -> ScanReport {
    let mut entries = BTreeMap::new();

    for root in config.system_roots.iter().chain(config.user_roots.iter()) {
        if !root.exists() {
            continue;
        }
        let walker = WalkDir::new(root).max_depth(config.max_depth).into_iter();
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    if let Some(io) = err.io_error() {
                        log::debug!("skipping entry while scanning {}: {}", root.display(), io);
                    }
                    continue;
                }
            };
            if let Some((format, candidate)) = classify_candidate(entry.path()) {
                let metadata = prober.probe(format, &candidate);
                let mut plugin_entry = metadata
                    .map(|metadata| metadata.into_entry(format, candidate.display().to_string()))
                    .unwrap_or_else(|_| {
                        let mut stub = stub_metadata(format, &candidate)
                            .into_entry(format, candidate.display().to_string());
                        stub.quarantined = true;
                        stub
                    });
                plugin_entry.last_seen = Utc::now();
                entries.insert(plugin_entry.reference.clone(), plugin_entry);
            }
        }
    }

    ScanReport {
        entries: entries.into_values().collect(),
    }
}

fn classify_candidate(path: &Path) -> Option<(PluginFormat, PathBuf)> {
    if path.is_file() {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("clap") => return Some((PluginFormat::Clap, path.to_path_buf())),
            Some("hqplug") => return Some((PluginFormat::Harmoniq, path.to_path_buf())),
            _ => {}
        }
        if let Some(parent) = path.parent() {
            if path.file_name()?.eq("plugin.clap") {
                return Some((PluginFormat::Clap, parent.to_path_buf()));
            }
        }
    } else if path.is_dir() {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if name.ends_with(".clap") {
                return Some((PluginFormat::Clap, path.to_path_buf()));
            }
            if name.ends_with(".vst3") {
                return Some((PluginFormat::Vst3, path.to_path_buf()));
            }
            if name.ends_with(".ovst3") {
                return Some((PluginFormat::Ovst3, path.to_path_buf()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, File};
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn scan_discovers_plugins_from_all_roots() {
        let dir = tempdir().unwrap();
        let sys_clap = dir.path().join("system/clap");
        let user_vst3 = dir.path().join("user/.vst3");
        create_dir_all(&sys_clap).unwrap();
        create_dir_all(&user_vst3).unwrap();
        let clap_plugin = sys_clap.join("synth.clap");
        File::create(&clap_plugin).unwrap();
        let vst3_plugin = user_vst3.join("Cool.vst3");
        create_dir_all(&vst3_plugin).unwrap();
        let manifest = vst3_plugin.join("manifest.json");
        let mut file = File::create(manifest).unwrap();
        write!(
            file,
            "{}",
            serde_json::json!({
                "id": "cool.vst3",
                "name": "Cool",
                "vendor": "Acme",
                "is_instrument": true,
                "has_editor": true,
                "num_inputs": 2,
                "num_outputs": 2
            })
            .to_string()
        )
        .unwrap();

        let mut config = ScanConfig::default();
        config.system_roots = vec![sys_clap.clone()];
        config.user_roots = vec![user_vst3.parent().unwrap().to_path_buf()];

        let report = scan_plugins(&config, &ManifestProber::default());
        assert_eq!(report.entries.len(), 2);
        let names: Vec<_> = report.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"synth"));
        assert!(names.contains(&"Cool"));
    }

    #[test]
    fn classify_finds_contents_plugin_clap_parent() {
        let dir = tempdir().unwrap();
        let bundle = dir.path().join("Bundle.clap/Contents");
        create_dir_all(&bundle).unwrap();
        let plugin = bundle.join("plugin.clap");
        File::create(&plugin).unwrap();
        let result = classify_candidate(&plugin);
        assert_eq!(
            result,
            Some((PluginFormat::Clap, dir.path().join("Bundle.clap")))
        );
    }
}
