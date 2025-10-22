use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::audio_buffer::AudioBuffer;
use crate::discovery::{discover_plugins, DiscoveredPlugin, PluginFormat};
use crate::editor::{create_egui_handle, EditorCommand, EditorEvent, PluginEditorHandle};
use crate::error::HostError;
use crate::parameters::{
    create_parameter_automation, AutomationMessage, ParameterAutomationChannels, PluginParam,
};
use crossbeam_channel::{Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginId(pub u64);

impl PluginId {
    fn next(counter: &AtomicU64) -> Self {
        PluginId(counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

/// Trait implemented by unified plugin host backends.
pub trait PluginHost {
    fn load_plugin(&mut self, path: &Path) -> Result<PluginId, HostError>;
    fn unload_plugin(&mut self, id: PluginId);
    fn process(&mut self, inputs: &[AudioBuffer], outputs: &mut [AudioBuffer], frames: usize);
    fn get_parameters(&self) -> Vec<PluginParam>;
    fn set_parameter(&mut self, index: usize, value: f32);
    fn editor(&mut self) -> Option<PluginEditorHandle>;
}

/// Unified host capable of managing VST3, LV2, CLAP, and Harmoniq plugins.
pub struct UnifiedPluginHost {
    next_id: AtomicU64,
    plugins: HashMap<PluginId, LoadedPlugin>,
    discovery: Vec<DiscoveredPlugin>,
    active_plugin: Option<PluginId>,
}

struct LoadedPlugin {
    id: PluginId,
    path: PathBuf,
    format: PluginFormat,
    parameters: Vec<PluginParam>,
    automation: Vec<ParameterAutomationChannels>,
    editor: Option<PluginEditorHandle>,
    editor_channels: Option<EditorChannelState>,
}

struct EditorChannelState {
    commands: Receiver<EditorCommand>,
    events: Sender<EditorEvent>,
}

impl UnifiedPluginHost {
    pub fn new() -> Self {
        let discovery = discover_plugins().plugins;
        Self {
            next_id: AtomicU64::new(0),
            plugins: HashMap::new(),
            discovery,
            active_plugin: None,
        }
    }

    pub fn discovered_plugins(&self) -> &[DiscoveredPlugin] {
        &self.discovery
    }

    pub fn activate(&mut self, id: PluginId) {
        if self.plugins.contains_key(&id) {
            self.active_plugin = Some(id);
        }
    }

    fn active_plugin_mut(&mut self) -> Option<&mut LoadedPlugin> {
        let id = self.active_plugin?;
        self.plugins.get_mut(&id)
    }
}

impl PluginHost for UnifiedPluginHost {
    fn load_plugin(&mut self, path: &Path) -> Result<PluginId, HostError> {
        let path = path.to_path_buf();
        if !path.exists() {
            return Err(HostError::MissingBinary(path));
        }

        let format = detect_format(&path).ok_or_else(|| {
            HostError::Unsupported(format!(
                "Unable to detect plugin format for {}",
                path.display()
            ))
        })?;

        let id = PluginId::next(&self.next_id);
        let mut parameters = Vec::new();
        let mut automation_channels = Vec::new();

        for index in 0..4 {
            let (automation, channels) = create_parameter_automation();
            let param = PluginParam {
                index,
                id: format!("param_{index}"),
                name: format!("Parameter {index}"),
                value: 0.5,
                default: 0.5,
                min: 0.0,
                max: 1.0,
                automation,
            };
            parameters.push(param);
            automation_channels.push(channels);
        }

        let plugin = LoadedPlugin {
            id,
            path,
            format,
            parameters,
            automation: automation_channels,
            editor: None,
            editor_channels: None,
        };
        self.plugins.insert(id, plugin);
        self.active_plugin = Some(id);
        Ok(id)
    }

    fn unload_plugin(&mut self, id: PluginId) {
        self.plugins.remove(&id);
        if self.active_plugin == Some(id) {
            self.active_plugin = self.plugins.keys().copied().next();
        }
    }

    fn process(&mut self, inputs: &[AudioBuffer], outputs: &mut [AudioBuffer], frames: usize) {
        for plugin in self.plugins.values_mut() {
            for (param, channels) in plugin
                .parameters
                .iter_mut()
                .zip(plugin.automation.iter_mut())
            {
                while let Ok(message) = channels.to_engine_rx.try_recv() {
                    if let AutomationMessage::SetValue { value } = message {
                        param.value = value;
                    }
                }
            }
        }

        for (output, input) in outputs.iter_mut().zip(inputs.iter()) {
            output.resize(input.channels(), frames);
            for (dst, src) in output.channel_slices_mut().zip(input.channel_slices()) {
                dst[..frames.min(src.len())].copy_from_slice(&src[..frames.min(src.len())]);
            }
        }
        for output in outputs.iter_mut().skip(inputs.len()) {
            output.resize(2, frames);
            output.clear();
        }
    }

    fn get_parameters(&self) -> Vec<PluginParam> {
        self.active_plugin
            .and_then(|id| self.plugins.get(&id))
            .map(|plugin| plugin.parameters.clone())
            .unwrap_or_default()
    }

    fn set_parameter(&mut self, index: usize, value: f32) {
        if let Some(plugin) = self.active_plugin_mut() {
            if let Some(param) = plugin.parameters.get_mut(index) {
                param.value = value.clamp(param.min, param.max);
                if let Some(channels) = plugin.automation.get(index) {
                    let _ = channels
                        .from_engine_tx
                        .try_send(AutomationMessage::SetValue { value });
                }
            }
        }
    }

    fn editor(&mut self) -> Option<PluginEditorHandle> {
        let plugin = self.active_plugin_mut()?;
        if let Some(handle) = &plugin.editor {
            return Some(handle.clone());
        }

        let ctx = Arc::new(egui::Context::default());
        let (handle, command_rx, event_tx) = create_egui_handle(plugin.id, Arc::clone(&ctx));
        plugin.editor_channels = Some(EditorChannelState {
            commands: command_rx,
            events: event_tx,
        });
        plugin.editor = Some(handle.clone());
        Some(handle)
    }
}

fn detect_format(path: &Path) -> Option<PluginFormat> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".vst3"))
        .unwrap_or(false)
    {
        Some(PluginFormat::Vst3)
    } else if ext.eq_ignore_ascii_case("lv2") {
        Some(PluginFormat::Lv2)
    } else if ext.eq_ignore_ascii_case("clap") {
        Some(PluginFormat::Clap)
    } else if ext.eq_ignore_ascii_case("harmoniq") {
        Some(PluginFormat::Harmoniq)
    } else {
        None
    }
}

impl LoadedPlugin {
    pub fn editor_handle(&self) -> Option<&PluginEditorHandle> {
        self.editor.as_ref()
    }

    pub fn parameters(&self) -> &[PluginParam] {
        &self.parameters
    }
}
