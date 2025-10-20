use std::{
    ffi::c_void,
    fmt,
    path::{Path, PathBuf},
};

use libloading::Library;

use crate::HostError;

/// Signature of the VST entry point exposed by LinuxVST binaries.
pub type LinuxVstEntryPoint = unsafe extern "C" fn() -> *mut c_void;

/// Host wrapper for LinuxVST dynamic libraries.
pub struct LinuxVstHost {
    library_path: PathBuf,
    library: Library,
    entry: LinuxVstEntryPoint,
}

impl LinuxVstHost {
    /// Load a LinuxVST binary and resolve the `VSTPluginMain` entry point.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HostError::MissingBinary(path.to_path_buf()));
        }

        let library = unsafe { Library::new(path) }?;
        let entry_symbol = unsafe {
            library
                .get::<LinuxVstEntryPoint>(b"VSTPluginMain\0")
                .or_else(|_| library.get::<LinuxVstEntryPoint>(b"main\0"))
                .map_err(|_| HostError::missing_entry(path.to_path_buf(), "VSTPluginMain"))?
        };
        let entry = *entry_symbol;

        Ok(Self {
            library_path: path.to_path_buf(),
            library,
            entry,
        })
    }

    /// Access the resolved entry point.
    pub fn entry_point(&self) -> LinuxVstEntryPoint {
        self.entry
    }

    /// Path to the backing dynamic library.
    pub fn path(&self) -> &Path {
        &self.library_path
    }

    /// Borrow the underlying library handle to keep the binary loaded.
    pub fn library(&self) -> &Library {
        &self.library
    }
}

impl fmt::Debug for LinuxVstHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxVstHost")
            .field("library_path", &self.library_path)
            .finish()
    }
}
