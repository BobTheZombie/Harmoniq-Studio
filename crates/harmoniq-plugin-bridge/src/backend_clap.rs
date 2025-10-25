use clap_host::{ClapLibrary, PluginDiscovery};

/// Minimal CLAP backend used by the plug-in bridge.
pub fn enumerate(path: &std::path::Path) -> anyhow::Result<Vec<String>> {
    unsafe {
        let library = ClapLibrary::load(path)?;
        let factory = library.factory()?;
        let discovery = PluginDiscovery::new(factory);
        Ok(discovery
            .list()
            .into_iter()
            .map(|descriptor| descriptor.id)
            .collect())
    }
}
