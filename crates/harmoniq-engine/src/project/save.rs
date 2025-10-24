use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::schema::{
    MediaChunkDescriptor, ProjectDocument, ProjectMediaEntryV2, ProjectV2, CURRENT_VERSION,
    MEDIA_CHUNK_SIZE, PROJECT_MAGIC,
};

#[derive(Debug, Clone)]
pub struct SaveOptions {
    pub remove_autosave: bool,
    pub chunk_size: usize,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            remove_autosave: true,
            chunk_size: MEDIA_CHUNK_SIZE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SaveReport {
    pub path: PathBuf,
    pub bytes_written: u64,
    pub media_bytes: u64,
    pub autosave: bool,
}

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("io error while saving project: {0}")]
    Io(#[from] io::Error),
    #[error("project payload too large")]
    ProjectTooLarge,
}

pub fn save_project(
    path: &Path,
    document: &ProjectDocument,
    options: SaveOptions,
) -> Result<SaveReport, SaveError> {
    write_archive(path, document, options, false)
}

pub fn save_autosave(path: &Path, document: &ProjectDocument) -> Result<SaveReport, SaveError> {
    let autosave_path = autosave_path(path);
    let mut options = SaveOptions::default();
    options.remove_autosave = false;
    write_archive(&autosave_path, document, options, true)
}

pub fn autosave_path(path: &Path) -> PathBuf {
    if path.extension().is_some() {
        let mut new = path.as_os_str().to_owned();
        new.push(".autosave");
        PathBuf::from(new)
    } else {
        path.with_extension("autosave")
    }
}

fn write_archive(
    path: &Path,
    document: &ProjectDocument,
    options: SaveOptions,
    autosave: bool,
) -> Result<SaveReport, SaveError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let chunk_size = options.chunk_size.max(1);
    let mut chunk_data = Vec::new();
    let mut entries = Vec::with_capacity(document.media.len());

    for asset in &document.media {
        let mut descriptors = Vec::new();
        if asset.data.is_empty() {
            descriptors.push(MediaChunkDescriptor {
                offset: chunk_data.len() as u64,
                length: 0,
            });
        } else {
            let mut cursor = 0usize;
            while cursor < asset.data.len() {
                let end = (cursor + chunk_size).min(asset.data.len());
                let slice = &asset.data[cursor..end];
                let offset = chunk_data.len() as u64;
                chunk_data.extend_from_slice(slice);
                descriptors.push(MediaChunkDescriptor {
                    offset,
                    length: slice.len() as u32,
                });
                cursor = end;
            }
        }

        entries.push(ProjectMediaEntryV2 {
            id: asset.id.clone(),
            relative_path: asset.relative_path.clone(),
            checksum: asset.checksum.clone(),
            chunks: descriptors,
        });
    }

    let project = ProjectV2::new(document.metadata.clone(), entries);
    let json = serde_json::to_vec_pretty(&project).map_err(|_| SaveError::ProjectTooLarge)?;

    let json_len = u32::try_from(json.len()).map_err(|_| SaveError::ProjectTooLarge)?;
    let media_len = u64::try_from(chunk_data.len()).map_err(|_| SaveError::ProjectTooLarge)?;

    let tmp_path = path.with_extension("tmp");
    let mut file = File::create(&tmp_path)?;
    file.write_all(&PROJECT_MAGIC)?;
    file.write_all(&CURRENT_VERSION.to_le_bytes())?;
    file.write_all(&json_len.to_le_bytes())?;
    file.write_all(&media_len.to_le_bytes())?;
    file.write_all(&json)?;
    file.write_all(&chunk_data)?;
    file.flush()?;
    drop(file);

    fs::rename(&tmp_path, path)?;

    if options.remove_autosave && !autosave {
        let autosave_path = autosave_path(path);
        if autosave_path.exists() {
            let _ = fs::remove_file(autosave_path);
        }
    }

    Ok(SaveReport {
        path: path.to_path_buf(),
        bytes_written: (json.len() + chunk_data.len() + 20) as u64,
        media_bytes: media_len,
        autosave,
    })
}
