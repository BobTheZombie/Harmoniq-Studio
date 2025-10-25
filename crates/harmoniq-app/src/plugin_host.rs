use std::cell::RefCell;
use std::path::Path;

use anyhow::{Context, Result};
use harmoniq_plugin_host::PluginHost;
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

std::thread_local! {
    static HOST: RefCell<HostState> = RefCell::new(HostState::new());
}

/// Loads a plugin binary using the shared `UnifiedPluginHost` instance.
///
/// This helper keeps host management out of the egui UI code while ensuring
/// that plugin loading never touches the realtime audio thread. The host is
/// kept in a thread-local `RefCell` because plugin host handles are not `Send`
/// and all UI interactions occur on the main thread.
pub fn load_plugin(path: &Path) -> Result<PluginId> {
    HOST.with(|state| {
        let mut state = state.borrow_mut();
        let id = state
            .host
            .load_plugin(path)
            .with_context(|| format!("failed to load plugin at {}", path.display()))?;
        state.host.activate(id);
        state.loaded.push(id);
        Ok(id)
    })
}

/// Unloads a previously loaded plugin.
pub fn unload_plugin(id: PluginId) {
    HOST.with(|state| {
        let mut state = state.borrow_mut();
        state.host.unload_plugin(id);
        if let Some(pos) = state.loaded.iter().position(|loaded| *loaded == id) {
            state.loaded.swap_remove(pos);
        }
    });
}

/// Returns the number of loaded plugins tracked by the shared host.
pub fn loaded_count() -> usize {
    HOST.with(|state| state.borrow().loaded.len())
}
