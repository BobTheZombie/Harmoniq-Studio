use std::{fmt, path::Path};

use crate::{HostError, PluginBinaryFormat};

#[cfg(target_os = "macos")]
use std::{ffi::c_void, path::PathBuf};

/// Signature of the AudioUnit component factory entry point.
#[cfg(target_os = "macos")]
pub type AudioUnitFactory = unsafe extern "C" fn() -> *mut c_void;
#[cfg(not(target_os = "macos"))]
pub type AudioUnitFactory = ();

#[cfg(target_os = "macos")]
use libloading::Library;

#[cfg(target_os = "macos")]
/// Host wrapper for Apple AudioUnit components (macOS).
pub struct AudioUnitHost {
    library_path: PathBuf,
    library: Library,
    factory: AudioUnitFactory,
}

#[cfg(target_os = "macos")]
impl AudioUnitHost {
    /// Load a component bundle's executable and resolve the factory entry point.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(HostError::MissingBinary(path.to_path_buf()));
        }

        let library = unsafe { Library::new(path) }?;
        let factory_symbol = unsafe {
            library
                .get::<AudioUnitFactory>(b"AudioComponentFactoryFunction\0")
                .or_else(|_| library.get::<AudioUnitFactory>(b"ComponentEntryPoint\0"))
                .map_err(|_| {
                    HostError::missing_entry(path.to_path_buf(), "AudioComponentFactoryFunction")
                })?
        };
        let factory = *factory_symbol;

        Ok(Self {
            library_path: path.to_path_buf(),
            library,
            factory,
        })
    }

    /// Access the factory entry point.
    pub fn component_factory(&self) -> AudioUnitFactory {
        self.factory
    }

    /// Path to the component's executable image.
    pub fn path(&self) -> &Path {
        &self.library_path
    }

    /// Borrow the backing component image handle.
    pub fn library(&self) -> &Library {
        &self.library
    }
}

#[cfg(target_os = "macos")]
impl fmt::Debug for AudioUnitHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioUnitHost")
            .field("library_path", &self.library_path)
            .finish()
    }
}

#[cfg(not(target_os = "macos"))]
/// Placeholder implementation used on unsupported platforms.
pub struct AudioUnitHost;

#[cfg(not(target_os = "macos"))]
impl AudioUnitHost {
    /// AudioUnit hosting is only available on macOS.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
        let _ = path;
        Err(HostError::platform_unsupported(
            PluginBinaryFormat::AudioUnit,
        ))
    }

    /// Unsupported on non-macOS targets.
    pub fn component_factory(&self) -> AudioUnitFactory {
        ()
    }
}

#[cfg(not(target_os = "macos"))]
impl fmt::Debug for AudioUnitHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioUnitHost")
            .field("platform", &"unsupported")
            .finish()
    }
}
