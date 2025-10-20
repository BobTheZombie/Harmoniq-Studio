use std::{
    ffi::c_void,
    fmt,
    path::{Path, PathBuf},
};

use libloading::Library;

use crate::HostError;

/// Signature of the `GetPluginFactory` export defined by the VST3 SDK.
pub type Vst3FactoryEntry = unsafe extern "C" fn() -> *mut c_void;

/// Host wrapper for VST3 plugin modules.
pub struct Vst3Host {
    library_path: PathBuf,
    library: Library,
    factory: Vst3FactoryEntry,
}

impl Vst3Host {
    /// Load a VST3 plugin module.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HostError::MissingBinary(path.to_path_buf()));
        }

        let library = unsafe { Library::new(path) }?;
        let factory_symbol = unsafe {
            library
                .get::<Vst3FactoryEntry>(b"GetPluginFactory\0")
                .map_err(|_| HostError::missing_entry(path.to_path_buf(), "GetPluginFactory"))?
        };
        let factory = *factory_symbol;

        Ok(Self {
            library_path: path.to_path_buf(),
            library,
            factory,
        })
    }

    /// Access the resolved factory function pointer.
    pub fn factory(&self) -> Vst3FactoryEntry {
        self.factory
    }

    /// Path to the plugin library.
    pub fn path(&self) -> &Path {
        &self.library_path
    }

    /// Borrow the dynamic library backing this plugin module.
    pub fn library(&self) -> &Library {
        &self.library
    }
}

impl fmt::Debug for Vst3Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vst3Host")
            .field("library_path", &self.library_path)
            .finish()
    }
}
