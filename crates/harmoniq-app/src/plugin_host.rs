use std::path::Path;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use harmoniq_plugin_host::{PluginId, UnifiedPluginHost};

struct HostState {
    host: UnifiedPluginHost,
    loaded: Vec<PluginId>,
}

impl HostState {
    fn new() -> Self {
        Self {
            host: UnifiedPluginHost::new(),
            loaded: Vec::new(),
        }
    }
}

static HOST: Lazy<Mutex<HostState>> = Lazy::new(|| Mutex::new(HostState::new()));

/// Loads a plugin binary using the shared `UnifiedPluginHost` instance.
///
/// This helper keeps host management out of the egui UI code while ensuring
/// that plugin loading never touches the realtime audio thread. The host is
/// protected by a `Mutex` because loading is a potentially blocking operation
/// (file I/O, dynamic linking, etc.).
pub fn load_plugin(path: &Path) -> Result<PluginId> {
    let mut state = HOST.lock();
    let id = state
        .host
        .load_plugin(path)
        .with_context(|| format!("failed to load plugin at {}", path.display()))?;
    state.host.activate(id);
    state.loaded.push(id);
    Ok(id)
}

/// Unloads a previously loaded plugin.
pub fn unload_plugin(id: PluginId) {
    let mut state = HOST.lock();
    state.host.unload_plugin(id);
    if let Some(pos) = state.loaded.iter().position(|loaded| *loaded == id) {
        state.loaded.swap_remove(pos);
    }
}

/// Returns the number of loaded plugins tracked by the shared host.
pub fn loaded_count() -> usize {
    let state = HOST.lock();
    state.loaded.len()
}
