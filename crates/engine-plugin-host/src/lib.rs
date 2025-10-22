use anyhow::{anyhow, Result};
use engine_graph::AudioNode;
use harmoniq_plugin_sdk::{
    take_factory, PluginDescriptor, PluginFactory, PluginInstance, ENTRY_SYMBOL,
};
use libloading::Library;
use std::path::Path;
use std::sync::Arc;

pub struct PluginLibrary {
    library: Arc<Library>,
    factory: Box<dyn PluginFactory>,
    descriptor: PluginDescriptor,
}

impl PluginLibrary {
    pub unsafe fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let library = Library::new(&path_buf)
            .map_err(|err| anyhow!("failed to load plugin {path_buf:?}: {err}"))?;
        let entry = library
            .get::<harmoniq_plugin_sdk::PluginEntry>(ENTRY_SYMBOL.as_bytes())
            .map_err(|err| anyhow!("missing entry symbol in {path_buf:?}: {err}"))?;
        let factory = take_factory(*entry);
        let descriptor = factory.descriptor().clone();
        Ok(Self {
            library: Arc::new(library),
            factory,
            descriptor,
        })
    }

    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub fn create_instance(&self) -> HostedPlugin {
        HostedPlugin {
            library: self.library.clone(),
            descriptor: self.descriptor.clone(),
            instance: self.factory.create(),
        }
    }
}

pub struct HostedPlugin {
    #[allow(dead_code)]
    library: Arc<Library>,
    descriptor: PluginDescriptor,
    instance: Box<dyn PluginInstance>,
}

impl HostedPlugin {
    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub fn node(&mut self) -> &mut dyn AudioNode {
        self.instance.node()
    }
}

pub struct PluginHost {
    libraries: Vec<PluginLibrary>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            libraries: Vec::new(),
        }
    }

    pub unsafe fn load(&mut self, path: impl AsRef<Path>) -> Result<&PluginLibrary> {
        let library = PluginLibrary::load(path)?;
        self.libraries.push(library);
        Ok(self.libraries.last().expect("just pushed"))
    }

    pub fn libraries(&self) -> &[PluginLibrary] {
        &self.libraries
    }
}
