use std::{
    ffi::c_void,
    fmt,
    path::{Path, PathBuf},
};

use libloading::Library;

use crate::HostError;

/// Signature of the canonical VST2 entry point (`VSTPluginMain`).
pub type Vst2EntryPoint = unsafe extern "C" fn() -> *mut c_void;

/// Host wrapper for Steinberg VST2 plugins.
pub struct Vst2Host {
    library_path: PathBuf,
    library: Library,
    entry: Vst2EntryPoint,
}

impl Vst2Host {
    /// Load a VST2 plugin and resolve the exported entry point.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HostError::MissingBinary(path.to_path_buf()));
        }

        let library = unsafe { Library::new(path) }?;
        let entry_symbol = unsafe {
            library
                .get::<Vst2EntryPoint>(b"VSTPluginMain\0")
                .or_else(|_| library.get::<Vst2EntryPoint>(b"main\0"))
                .map_err(|_| HostError::missing_entry(path.to_path_buf(), "VSTPluginMain"))?
        };
        let entry = *entry_symbol;

        Ok(Self {
            library_path: path.to_path_buf(),
            library,
            entry,
        })
    }

    /// Access the plugin entry point function pointer.
    pub fn entry_point(&self) -> Vst2EntryPoint {
        self.entry
    }

    /// Path to the plugin dynamic library.
    pub fn path(&self) -> &Path {
        &self.library_path
    }

    /// Borrow the underlying dynamic library handle.
    pub fn library(&self) -> &Library {
        &self.library
    }
}

impl fmt::Debug for Vst2Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vst2Host")
            .field("library_path", &self.library_path)
            .finish()
    }
}
