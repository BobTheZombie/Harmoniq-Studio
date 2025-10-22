use std::path::PathBuf;

use thiserror::Error;

use crate::PluginFormat;

/// Errors that can occur while loading or managing plugins.
#[derive(Debug, Error)]
pub enum HostError {
    #[error("plugin binary not found at {0}")]
    MissingBinary(PathBuf),
    #[error("failed to load plugin library: {0}")]
    LibraryLoad(#[from] libloading::Error),
    #[error("{format:?} hosting is not available on this platform")]
    PlatformUnsupported { format: PluginFormat },
    #[error("unsupported plugin: {0}")]
    Unsupported(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid plugin state: {0}")]
    InvalidState(String),
}
