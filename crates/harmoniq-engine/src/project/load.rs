use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use thiserror::Error;

use super::migrate;
use super::save::autosave_path;
use super::schema::{
    MediaAsset, MediaChecksum, ProjectDocument, ProjectMediaEntryV2, ProjectV1, ProjectV2,
    ProjectV3, PROJECT_MAGIC,
};

pub type RelinkerCallback<'a> = dyn for<'r> FnMut(RelinkRequest<'r>) -> Option<PathBuf> + 'a;

pub struct LoadOptions<'a> {
    pub prefer_autosave: bool,
    pub relinker: Option<Box<RelinkerCallback<'a>>>,
}

impl<'a> Default for LoadOptions<'a> {
    fn default() -> Self {
        Self {
            prefer_autosave: true,
            relinker: None,
        }
    }
}

#[derive(Debug)]
pub struct RelinkRequest<'a> {
    pub id: &'a str,
    pub relative_path: &'a Path,
    pub checksum: &'a MediaChecksum,
    pub data: &'a [u8],
}

#[derive(Debug)]
pub struct LoadReport {
    pub document: ProjectDocument,
    pub recovered_from_autosave: bool,
    pub source_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("io error while loading project: {0}")]
    Io(#[from] io::Error),
    #[error("failed to parse project file: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("unsupported project version {0}")]
    UnsupportedVersion(u32),
    #[error("corrupt project file: {0}")]
    Corrupt(&'static str),
    #[error("checksum mismatch for media asset {id}")]
    CorruptMedia { id: String },
    #[error("missing media asset {id} at {path:?}")]
    MissingMedia { id: String, path: PathBuf },
    #[error("project migration failed: {0}")]
    Migration(String),
}

pub fn load_project(path: &Path, mut options: LoadOptions<'_>) -> Result<LoadReport, LoadError> {
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let autosave = autosave_path(path);

    if options.prefer_autosave && should_use_autosave(path, &autosave) {
        if autosave.exists() {
            let document =
                load_from_file(&autosave, path, base_dir, options.relinker.as_deref_mut())?;
            return Ok(LoadReport {
                document,
                recovered_from_autosave: true,
                source_path: autosave,
            });
        }
    }

    let document = load_from_file(path, path, base_dir, options.relinker.as_deref_mut())?;
    Ok(LoadReport {
        document,
        recovered_from_autosave: false,
        source_path: path.to_path_buf(),
    })
}

fn should_use_autosave(primary: &Path, autosave: &Path) -> bool {
    if !autosave.exists() {
        return false;
    }
    match (metadata_modified(primary), metadata_modified(autosave)) {
        (None, Some(_)) => true,
        (Some(primary_time), Some(autosave_time)) => autosave_time >= primary_time,
        _ => false,
    }
}

fn metadata_modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

fn load_from_file(
    path: &Path,
    source_path: &Path,
    base_dir: &Path,
    relinker: Option<&mut RelinkerCallback<'_>>,
) -> Result<ProjectDocument, LoadError> {
    let mut buffer = Vec::new();
    let mut file = File::open(path)?;
    file.read_to_end(&mut buffer)?;
    parse_buffer(&buffer, source_path, base_dir, relinker)
}

fn parse_buffer(
    buffer: &[u8],
    source_path: &Path,
    base_dir: &Path,
    relinker: Option<&mut RelinkerCallback<'_>>,
) -> Result<ProjectDocument, LoadError> {
    if buffer.starts_with(&PROJECT_MAGIC) {
        parse_archive(buffer, base_dir, relinker)
    } else {
        let project: ProjectV1 = serde_json::from_slice(buffer)?;
        migrate::from_v1(project, source_path, base_dir)
            .map_err(|err| LoadError::Migration(err.to_string()))
    }
}

fn parse_archive(
    buffer: &[u8],
    base_dir: &Path,
    mut relinker: Option<&mut RelinkerCallback<'_>>,
) -> Result<ProjectDocument, LoadError> {
    if buffer.len() < 20 {
        return Err(LoadError::Corrupt("truncated header"));
    }

    let version = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
    let json_len = u32::from_le_bytes(buffer[8..12].try_into().unwrap()) as usize;
    let media_len = u64::from_le_bytes(buffer[12..20].try_into().unwrap()) as usize;
    if buffer.len() < 20 + json_len + media_len {
        return Err(LoadError::Corrupt("payload truncated"));
    }

    let json_slice = &buffer[20..20 + json_len];
    let media_slice = &buffer[20 + json_len..20 + json_len + media_len];
    let load_media = |entries: Vec<ProjectMediaEntryV2>| {
        let mut media_assets = Vec::with_capacity(entries.len());
        for entry in entries {
            let ProjectMediaEntryV2 {
                id,
                relative_path,
                checksum,
                chunks,
            } = entry;

            let mut data = Vec::new();
            for chunk in chunks {
                let start = chunk.offset as usize;
                let end = start
                    .checked_add(chunk.length as usize)
                    .ok_or(LoadError::Corrupt("chunk overflow"))?;
                if end > media_slice.len() {
                    return Err(LoadError::Corrupt("chunk outside media region"));
                }
                if chunk.length > 0 {
                    data.extend_from_slice(&media_slice[start..end]);
                }
            }

            if !checksum.validate(&data) {
                return Err(LoadError::CorruptMedia { id });
            }

            let mut asset = MediaAsset {
                id,
                relative_path,
                checksum,
                data,
                resolved_path: None,
            };
            let resolved = base_dir.join(&asset.relative_path);
            if resolved.exists() {
                asset.resolved_path = Some(resolved);
            } else if let Some(relinker_fn) = relinker.as_mut() {
                let relinker_fn = &mut **relinker_fn;
                let request = RelinkRequest {
                    id: &asset.id,
                    relative_path: &asset.relative_path,
                    checksum: &asset.checksum,
                    data: &asset.data,
                };
                if let Some(new_path) = (relinker_fn)(request) {
                    let absolute = if new_path.is_absolute() {
                        new_path
                    } else {
                        base_dir.join(&new_path)
                    };
                    asset.update_relative_path(base_dir, absolute);
                } else {
                    return Err(LoadError::MissingMedia {
                        id: asset.id,
                        path: asset.relative_path,
                    });
                }
            } else {
                return Err(LoadError::MissingMedia {
                    id: asset.id,
                    path: asset.relative_path,
                });
            }

            media_assets.push(asset);
        }
        Ok(media_assets)
    };

    match version {
        2 => {
            let project: ProjectV2 = serde_json::from_slice(json_slice)?;
            let media_assets = load_media(project.media)?;
            Ok(ProjectDocument::new(project.metadata, media_assets))
        }
        3 => {
            let project: ProjectV3 = serde_json::from_slice(json_slice)?;
            let media_assets = load_media(project.media)?;
            Ok(ProjectDocument::new(project.metadata, media_assets).with_state(project.state))
        }
        other => Err(LoadError::UnsupportedVersion(other)),
    }
}
