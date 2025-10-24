use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::state::ProjectState;

pub const PROJECT_MAGIC: [u8; 4] = *b"HSQ2";
pub const CURRENT_VERSION: u32 = 3;
pub const MEDIA_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub name: String,
    pub sample_rate: f32,
    pub block_size: usize,
    pub channels: u8,
    pub duration_seconds: f32,
}

impl ProjectMetadata {
    pub fn new(
        name: impl Into<String>,
        sample_rate: f32,
        block_size: usize,
        channels: u8,
        duration_seconds: f32,
    ) -> Self {
        Self {
            name: name.into(),
            sample_rate,
            block_size,
            channels,
            duration_seconds,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaAsset {
    pub id: String,
    pub relative_path: PathBuf,
    pub checksum: MediaChecksum,
    pub data: Vec<u8>,
    pub resolved_path: Option<PathBuf>,
}

impl MediaAsset {
    pub fn new(id: impl Into<String>, relative_path: impl Into<PathBuf>, data: Vec<u8>) -> Self {
        let checksum = MediaChecksum::from_data(&data);
        Self {
            id: id.into(),
            relative_path: relative_path.into(),
            checksum,
            data,
            resolved_path: None,
        }
    }

    pub fn with_resolved_path(mut self, path: PathBuf) -> Self {
        self.resolved_path = Some(path);
        self
    }

    pub fn update_relative_path(&mut self, base: &Path, new_path: PathBuf) {
        if new_path.is_absolute() {
            self.resolved_path = Some(new_path.clone());
            if let Ok(relative) = new_path.strip_prefix(base) {
                self.relative_path = relative.to_path_buf();
            } else {
                self.relative_path = new_path;
            }
        } else {
            let absolute = base.join(&new_path);
            self.resolved_path = Some(absolute);
            self.relative_path = new_path;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaChunkDescriptor {
    pub offset: u64,
    pub length: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaChecksum {
    pub algorithm: String,
    pub value: String,
}

impl MediaChecksum {
    pub fn from_data(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = hasher.finalize();
        Self {
            algorithm: "sha256".to_string(),
            value: hex::encode(digest),
        }
    }

    pub fn validate(&self, data: &[u8]) -> bool {
        match self.algorithm.as_str() {
            "sha256" => {
                let mut hasher = Sha256::new();
                hasher.update(data);
                let digest = hasher.finalize();
                hex::encode(digest) == self.value
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectDocument {
    pub version: u32,
    pub metadata: ProjectMetadata,
    pub media: Vec<MediaAsset>,
    pub state: ProjectState,
}

impl ProjectDocument {
    pub fn new(metadata: ProjectMetadata, media: Vec<MediaAsset>) -> Self {
        Self::with_state(metadata, media, ProjectState::default())
    }

    pub fn with_media(mut self, media: Vec<MediaAsset>) -> Self {
        self.media = media;
        self
    }

    pub fn with_state(mut self, state: ProjectState) -> Self {
        self.state = state;
        self.version = CURRENT_VERSION;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectV3 {
    pub version: u32,
    pub metadata: ProjectMetadata,
    pub media: Vec<ProjectMediaEntryV2>,
    pub state: ProjectState,
}

impl ProjectV3 {
    pub fn new(
        metadata: ProjectMetadata,
        media: Vec<ProjectMediaEntryV2>,
        state: ProjectState,
    ) -> Self {
        Self {
            version: CURRENT_VERSION,
            metadata,
            media,
            state,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectV2 {
    pub version: u32,
    pub metadata: ProjectMetadata,
    pub media: Vec<ProjectMediaEntryV2>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectMediaEntryV2 {
    pub id: String,
    pub relative_path: PathBuf,
    pub checksum: MediaChecksum,
    pub chunks: Vec<MediaChunkDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectV1 {
    pub name: String,
    pub sample_rate: f32,
    pub block_size: usize,
    pub channels: usize,
    pub duration_seconds: f32,
    #[serde(default)]
    pub media: Vec<ProjectMediaEntryV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectMediaEntryV1 {
    pub path: PathBuf,
}
