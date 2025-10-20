use std::{path::PathBuf, sync::Arc};

use thiserror::Error;

use crate::PluginBinaryFormat;

/// Errors that can occur while loading third-party plugin binaries.
#[derive(Debug, Error)]
pub enum HostError {
    /// The plugin binary could not be found on disk.
    #[error("plugin binary not found at {0}")]
    MissingBinary(PathBuf),
    /// Dynamic linking of the plugin failed.
    #[error("failed to load plugin library: {0}")]
    LibraryLoad(#[from] Arc<libloading::Error>),
    /// The binary did not expose the expected entry point symbol.
    #[error("missing `{symbol}` entry point in {library}")]
    MissingEntryPoint {
        /// Path to the plugin binary.
        library: PathBuf,
        /// Required symbol.
        symbol: &'static str,
    },
    /// The host platform does not support the requested plugin format.
    #[error("{format:?} hosting is not available on this platform")]
    PlatformUnsupported { format: PluginBinaryFormat },
}

impl HostError {
    pub(crate) fn missing_entry(library: PathBuf, symbol: &'static str) -> Self {
        HostError::MissingEntryPoint { library, symbol }
    }

    pub(crate) fn platform_unsupported(format: PluginBinaryFormat) -> Self {
        HostError::PlatformUnsupported { format }
    }
}

impl From<libloading::Error> for HostError {
    fn from(value: libloading::Error) -> Self {
        HostError::LibraryLoad(Arc::new(value))
    }
}
