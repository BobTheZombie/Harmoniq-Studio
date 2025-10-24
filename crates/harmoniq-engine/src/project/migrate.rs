use std::fs;
use std::path::Path;

use thiserror::Error;

use super::schema::{MediaAsset, ProjectDocument, ProjectMetadata, ProjectV1};

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("io error while migrating project: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid project schema: {0}")]
    Invalid(&'static str),
}

pub fn from_v1(
    project: ProjectV1,
    source_path: &Path,
    base_dir: &Path,
) -> Result<ProjectDocument, MigrationError> {
    let ProjectV1 {
        name,
        sample_rate,
        block_size,
        channels,
        duration_seconds,
        media,
    } = project;

    let channel_count: u8 = channels
        .try_into()
        .map_err(|_| MigrationError::Invalid("channel count exceeds supported range"))?;

    let metadata = ProjectMetadata::new(
        name,
        sample_rate,
        block_size,
        channel_count,
        duration_seconds,
    );

    let mut media_assets = Vec::with_capacity(media.len());
    let project_dir = if source_path
        .parent()
        .map(|p| p.as_os_str().is_empty())
        .unwrap_or(true)
    {
        base_dir.to_path_buf()
    } else {
        source_path.parent().unwrap().to_path_buf()
    };

    for (index, entry) in media.into_iter().enumerate() {
        let mut resolved = entry.path.clone();
        if resolved.is_relative() {
            resolved = project_dir.join(&resolved);
        }
        let data = fs::read(&resolved)?;
        let mut asset = MediaAsset::new(format!("media_{index}"), entry.path.clone(), data);
        asset.update_relative_path(base_dir, resolved);
        media_assets.push(asset);
    }

    Ok(ProjectDocument::new(metadata, media_assets))
}
