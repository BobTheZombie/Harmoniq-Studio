use std::path::Path;

use anyhow::Result;
use clap_host::{ClapLibrary, PluginDiscovery};

use crate::scan::ScannedPlugin;

pub fn probe_plugin(path: &Path) -> Result<Option<ScannedPlugin>> {
    unsafe {
        match ClapLibrary::load(path) {
            Ok(lib) => {
                if let Ok(factory) = lib.factory() {
                    let discovery = PluginDiscovery::new(factory);
                    if let Some(descriptor) = discovery.list().into_iter().next() {
                        return Ok(Some(ScannedPlugin {
                            path: path.to_path_buf(),
                            id: descriptor.id,
                            name: descriptor.name,
                            vendor: descriptor.vendor,
                        }));
                    }
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }
}
