use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use libloading::Library;

use crate::ffi::{clap_plugin_entry_t, clap_plugin_factory_t};

/// Represents a dynamically loaded CLAP library.
pub struct ClapLibrary {
    path: PathBuf,
    _lib: Library,
    entry: *const clap_plugin_entry_t,
    initialized: bool,
}

unsafe impl Send for ClapLibrary {}
unsafe impl Sync for ClapLibrary {}

impl ClapLibrary {
    pub unsafe fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let lib = Library::new(&path_buf)
            .with_context(|| format!("Failed to load CLAP library: {}", path_buf.display()))?;
        let entry_sym: libloading::Symbol<*const clap_plugin_entry_t> =
            lib.get(b"clap_entry").with_context(|| {
                format!("CLAP library missing entry symbol: {}", path_buf.display())
            })?;
        let entry = *entry_sym;
        if entry.is_null() {
            anyhow::bail!("CLAP library {} has null entry", path_buf.display());
        }

        let mut initialized = false;
        if let Some(init) = (*entry).init {
            let c_path = CString::new(path_buf.to_string_lossy().as_bytes()).unwrap();
            initialized = init(c_path.as_ptr());
            if !initialized {
                anyhow::bail!("CLAP entry init failed for {}", path_buf.display());
            }
        }

        Ok(Self {
            path: path_buf,
            _lib: lib,
            entry,
            initialized,
        })
    }

    pub fn factory(&self) -> Result<&'static clap_plugin_factory_t> {
        let factory_id = CString::new("clap.plugin-factory").unwrap();
        let get_factory = unsafe { (*self.entry).get_factory }
            .ok_or_else(|| anyhow::anyhow!("get_factory missing"))?;
        let ptr = unsafe { get_factory(factory_id.as_ptr()) } as *const clap_plugin_factory_t;
        if ptr.is_null() {
            anyhow::bail!("CLAP library {} returned null factory", self.path.display());
        }
        Ok(unsafe { &*ptr })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ClapLibrary {
    fn drop(&mut self) {
        unsafe {
            if self.initialized {
                if let Some(deinit) = (*self.entry).deinit {
                    deinit();
                }
            }
        }
    }
}

/// Lightweight description of a plug-in discovered in a CLAP library.
#[derive(Clone, Debug)]
pub struct ClapPluginDescriptor {
    pub id: String,
    pub name: String,
    pub vendor: String,
}

pub struct PluginDiscovery<'a> {
    factory: &'a clap_plugin_factory_t,
}

impl<'a> PluginDiscovery<'a> {
    pub fn new(factory: &'a clap_plugin_factory_t) -> Self {
        Self { factory }
    }

    pub fn list(self) -> Vec<ClapPluginDescriptor> {
        let Some(get_plugin_count) = self.factory.get_plugin_count else {
            return Vec::new();
        };
        let count = unsafe { get_plugin_count(self.factory) };
        let mut plugins = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Some(get_plugin_descriptor) = self.factory.get_plugin_descriptor else {
                break;
            };
            unsafe {
                let descriptor = get_plugin_descriptor(self.factory, index);
                if descriptor.is_null() {
                    continue;
                }
                let descriptor = &*descriptor;
                let to_string = |ptr: *const i8| -> String {
                    if ptr.is_null() {
                        return String::new();
                    }
                    CStr::from_ptr(ptr).to_string_lossy().into_owned()
                };
                plugins.push(ClapPluginDescriptor {
                    id: to_string(descriptor.id),
                    name: to_string(descriptor.name),
                    vendor: to_string(descriptor.vendor),
                });
            }
        }
        plugins
    }
}
