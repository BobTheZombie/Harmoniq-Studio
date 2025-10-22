use anyhow::{Context, Result};
use harmoniq_plugin_host::{
    discover_plugins, DiscoveredPlugin, PluginEditorHandle, PluginHost, PluginId, PluginParam,
    UnifiedPluginHost,
};
use std::collections::HashMap;

const EXTERNAL_ID_BASE: usize = 1_000_000;

#[derive(Debug, Clone)]
pub struct ExternalPluginSummary {
    pub host_id: PluginId,
    pub ui_id: usize,
    pub name: String,
    pub plugin_type: String,
    pub bypassed: bool,
    pub cpu: f32,
    pub latency_ms: f32,
}

#[derive(Debug)]
struct HostedPlugin {
    id: PluginId,
    info: DiscoveredPlugin,
    parameters: Vec<PluginParam>,
    bypassed: bool,
    open_editor: bool,
    editor: Option<PluginEditorHandle>,
    cpu: f32,
    latency_ms: f32,
}

impl HostedPlugin {
    fn new(id: PluginId, info: DiscoveredPlugin, parameters: Vec<PluginParam>) -> Self {
        Self {
            id,
            info,
            parameters,
            bypassed: false,
            open_editor: true,
            editor: None,
            cpu: 0.0,
            latency_ms: 0.0,
        }
    }

    fn summary(&self) -> ExternalPluginSummary {
        ExternalPluginSummary {
            host_id: self.id,
            ui_id: ExternalPluginManager::to_ui_id(self.id),
            name: self.info.name.clone(),
            plugin_type: format!("{} • {:?}", self.info.display_format(), self.info.category),
            bypassed: self.bypassed,
            cpu: self.cpu,
            latency_ms: self.latency_ms,
        }
    }
}

#[derive(Debug)]
pub struct ExternalPluginManager {
    host: UnifiedPluginHost,
    catalog: Vec<DiscoveredPlugin>,
    loaded: HashMap<PluginId, HostedPlugin>,
}

impl ExternalPluginManager {
    pub fn new() -> Self {
        let mut host = UnifiedPluginHost::new();
        let catalog = host.discovered_plugins().to_vec();
        Self {
            host,
            catalog,
            loaded: HashMap::new(),
        }
    }

    pub fn catalog(&self) -> &[DiscoveredPlugin] {
        &self.catalog
    }

    pub fn refresh_catalog(&mut self) {
        self.catalog = discover_plugins().plugins;
    }

    pub fn load(&mut self, plugin: &DiscoveredPlugin) -> Result<PluginId> {
        let id = self
            .host
            .load_plugin(&plugin.path)
            .with_context(|| format!("Failed to load plugin from {}", plugin.path.display()))?;
        self.host.activate(id);
        let parameters = self.host.get_parameters();
        let hosted = HostedPlugin::new(id, plugin.clone(), parameters);
        self.loaded.insert(id, hosted);
        Ok(id)
    }

    pub fn unload(&mut self, id: PluginId) {
        self.loaded.remove(&id);
        self.host.unload_plugin(id);
    }

    pub fn summaries(&self) -> Vec<ExternalPluginSummary> {
        self.loaded
            .values()
            .map(|plugin| plugin.summary())
            .collect()
    }

    pub fn open_editor(&mut self, id: PluginId) -> Option<PluginEditorHandle> {
        let plugin = self.loaded.get_mut(&id)?;
        self.host.activate(id);
        if plugin.editor.is_none() {
            plugin.editor = self.host.editor();
        }
        plugin.open_editor = true;
        plugin.editor.clone()
    }

    pub fn set_bypassed(&mut self, id: PluginId, bypassed: bool) {
        if let Some(plugin) = self.loaded.get_mut(&id) {
            plugin.bypassed = bypassed;
        }
    }

    pub fn parameters(&mut self, id: PluginId) -> Option<&mut [PluginParam]> {
        let plugin = self.loaded.get_mut(&id)?;
        self.host.activate(id);
        plugin.parameters = self.host.get_parameters();
        Some(plugin.parameters.as_mut_slice())
    }

    pub fn set_parameter(&mut self, id: PluginId, index: usize, value: f32) {
        if let Some(plugin) = self.loaded.get_mut(&id) {
            self.host.activate(id);
            self.host.set_parameter(index, value);
            plugin.parameters = self.host.get_parameters();
        }
    }

    pub fn to_ui_id(id: PluginId) -> usize {
        id.0 as usize + EXTERNAL_ID_BASE
    }

    pub fn from_ui_id(id: usize) -> Option<PluginId> {
        if id >= EXTERNAL_ID_BASE {
            Some(PluginId((id - EXTERNAL_ID_BASE) as u64))
        } else {
            None
        }
    }

    pub fn host_id(&self, id: PluginId) -> Option<&HostedPlugin> {
        self.loaded.get(&id)
    }

    pub fn host_id_mut(&mut self, id: PluginId) -> Option<&mut HostedPlugin> {
        self.loaded.get_mut(&id)
    }

    pub fn loaded_ids(&self) -> Vec<PluginId> {
        self.loaded.keys().copied().collect()
    }

    pub fn editor_metadata(&self, id: PluginId) -> Option<(String, String, bool)> {
        self.loaded.get(&id).map(|plugin| {
            (
                plugin.info.name.clone(),
                plugin.info.display_format().to_string(),
                plugin.open_editor,
            )
        })
    }

    pub fn set_editor_open(&mut self, id: PluginId, open: bool) {
        if let Some(plugin) = self.loaded.get_mut(&id) {
            plugin.open_editor = open;
        }
    }
}

pub fn is_external_plugin_id(id: usize) -> bool {
    id >= EXTERNAL_ID_BASE
}

pub fn external_id_from_ui(id: usize) -> Option<PluginId> {
    ExternalPluginManager::from_ui_id(id)
}

pub fn ui_id_from_external(id: PluginId) -> usize {
    ExternalPluginManager::to_ui_id(id)
}
