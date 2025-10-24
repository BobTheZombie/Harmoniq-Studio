use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROJECT_MAGIC: [u8; 4] = *b"HSQ2";
pub const CURRENT_VERSION: u32 = 2;
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
}

impl ProjectDocument {
    pub fn new(metadata: ProjectMetadata, media: Vec<MediaAsset>) -> Self {
        Self {
            version: CURRENT_VERSION,
            metadata,
            media,
        }
    }

    pub fn with_media(mut self, media: Vec<MediaAsset>) -> Self {
        self.media = media;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectV2 {
    pub version: u32,
    pub metadata: ProjectMetadata,
    pub media: Vec<ProjectMediaEntryV2>,
}

impl ProjectV2 {
    pub fn new(metadata: ProjectMetadata, media: Vec<ProjectMediaEntryV2>) -> Self {
        Self {
            version: CURRENT_VERSION,
            metadata,
            media,
        }
    }
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
