use std::env;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Identifies the boundary used to communicate with a VST3 plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AdapterKind {
    /// Use the official Steinberg VST3 SDK (linked from a broker process).
    OfficialSdk,
    /// Use the community maintained OpenVST3 shim implementation.
    OpenVst3Shim,
}

/// Configuration describing how the sandbox should instantiate the adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterDescriptor {
    /// The adapter flavor.
    pub kind: AdapterKind,
    /// Optional library path overriding the default loader for the adapter.
    pub library_path: Option<PathBuf>,
    /// Arbitrary extra arguments forwarded to the sandbox.
    pub extra_args: Vec<String>,
}

impl AdapterDescriptor {
    /// Creates a descriptor using the official Steinberg SDK with optional override path.
    pub fn official_sdk_with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: AdapterKind::OfficialSdk,
            library_path: Some(path.into()),
            extra_args: Vec::new(),
        }
    }

    /// Creates a descriptor pointing at the OpenVST3 shim with optional override path.
    pub fn open_vst3_shim_with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: AdapterKind::OpenVst3Shim,
            library_path: Some(path.into()),
            extra_args: Vec::new(),
        }
    }

    /// Creates a descriptor using the official Steinberg SDK relying on discovery.
    pub fn official_sdk() -> Self {
        Self {
            kind: AdapterKind::OfficialSdk,
            library_path: default_adapter_path(AdapterKind::OfficialSdk),
            extra_args: Vec::new(),
        }
    }

    /// Creates a descriptor using the OpenVST3 shim relying on discovery.
    pub fn open_vst3_shim() -> Self {
        Self {
            kind: AdapterKind::OpenVst3Shim,
            library_path: default_adapter_path(AdapterKind::OpenVst3Shim),
            extra_args: Vec::new(),
        }
    }

    /// Adds an additional argument to be forwarded to the sandbox.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Sets/overrides the library path.
    pub fn with_library_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.library_path = Some(path.into());
        self
    }

    /// Resolves the library path applying environment overrides when present.
    pub fn resolved_library_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.library_path {
            return Some(path.clone());
        }

        let env_key = match self.kind {
            AdapterKind::OfficialSdk => "HARMONIQ_VST3_SDK_ADAPTER",
            AdapterKind::OpenVst3Shim => "HARMONIQ_VST3_OPEN_ADAPTER",
        };

        env::var_os(env_key).map(PathBuf::from)
    }

    /// Returns a human readable name for diagnostics.
    pub fn label(&self) -> &'static str {
        match self.kind {
            AdapterKind::OfficialSdk => "Official SDK",
            AdapterKind::OpenVst3Shim => "OpenVST3 Shim",
        }
    }
}

fn default_adapter_path(kind: AdapterKind) -> Option<PathBuf> {
    let env_key = match kind {
        AdapterKind::OfficialSdk => "HARMONIQ_VST3_SDK_ADAPTER",
        AdapterKind::OpenVst3Shim => "HARMONIQ_VST3_OPEN_ADAPTER",
    };
    env::var_os(env_key).map(PathBuf::from)
}

/// Request issued to the sandbox describing how to load the VST3 binary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxRequest {
    /// Absolute path to the plugin bundle on disk.
    pub plugin_path: PathBuf,
    /// Adapter configuration to use for bridging the VST3 ABI boundary.
    pub adapter: AdapterDescriptor,
}

impl SandboxRequest {
    pub fn new(plugin_path: impl AsRef<Path>, adapter: AdapterDescriptor) -> Self {
        Self {
            plugin_path: plugin_path.as_ref().to_path_buf(),
            adapter,
        }
    }
}
