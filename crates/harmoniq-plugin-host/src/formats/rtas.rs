use std::{fmt, path::Path};

use crate::{HostError, PluginBinaryFormat};

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{ffi::c_void, path::PathBuf};

/// Signature of the RTAS descriptor export.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub type RtasEntryPoint = unsafe extern "C" fn(*mut c_void) -> i32;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub type RtasEntryPoint = ();

#[cfg(any(target_os = "windows", target_os = "macos"))]
use libloading::Library;

#[cfg(any(target_os = "windows", target_os = "macos"))]
/// Host wrapper for RTAS plugin libraries.
pub struct RtasHost {
    library_path: PathBuf,
    library: Library,
    entry: RtasEntryPoint,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl RtasHost {
    /// Load a RTAS plugin binary and resolve the descriptor entry point.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HostError::MissingBinary(path.to_path_buf()));
        }

        let library = unsafe { Library::new(path) }?;
        let entry_symbol = unsafe {
            library
                .get::<RtasEntryPoint>(b"GetEffectDescriptions\0")
                .or_else(|_| library.get::<RtasEntryPoint>(b"AAX_GetEffectDescriptions\0"))
                .map_err(|_| {
                    HostError::missing_entry(path.to_path_buf(), "GetEffectDescriptions")
                })?
        };
        let entry = *entry_symbol;

        Ok(Self {
            library_path: path.to_path_buf(),
            library,
            entry,
        })
    }

    /// Access the descriptor enumeration entry point.
    pub fn entry_point(&self) -> RtasEntryPoint {
        self.entry
    }

    /// Path to the plugin binary.
    pub fn path(&self) -> &Path {
        &self.library_path
    }

    /// Borrow the loaded RTAS binary handle.
    pub fn library(&self) -> &Library {
        &self.library
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl fmt::Debug for RtasHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtasHost")
            .field("library_path", &self.library_path)
            .finish()
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
/// Placeholder implementation for unsupported platforms.
pub struct RtasHost;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl RtasHost {
    /// RTAS hosting requires Windows or macOS.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let _ = path;
        Err(HostError::platform_unsupported(PluginBinaryFormat::Rtas))
    }

    /// Unsupported on this platform.
    pub fn entry_point(&self) -> RtasEntryPoint {
        ()
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl fmt::Debug for RtasHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtasHost")
            .field("platform", &"unsupported")
            .finish()
    }
}
