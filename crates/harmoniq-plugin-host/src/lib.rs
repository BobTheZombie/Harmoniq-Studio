//! Unified third-party plugin hosting for Harmoniq Studio.
//!
//! The crate exposes a format-agnostic [`PluginHost`] trait that abstracts the
//! life-cycle management, parameter automation, and editor embedding of VST3,
//! LV2, CLAP, and native Harmoniq plugins. Host backends can use the provided
//! utilities for plugin discovery, parameter messaging, and egui-based editor
//! surfaces.

mod audio_buffer;
mod discovery;
mod editor;
mod error;
mod host;
mod parameters;

pub use audio_buffer::AudioBuffer;
pub use discovery::{
    discover_plugins, DiscoveredPlugin, DiscoveryResult, PluginCategory, PluginFormat,
};
pub use editor::{
    EditorCommand, EditorEvent, EguiEditorHandle, NativeEditorHandle, PluginEditorHandle,
    PluginEditorKind, SharedReceiver,
};
pub use error::HostError;
pub use host::{PluginHost, PluginId, UnifiedPluginHost};
pub use parameters::{AutomationMessage, PluginParam};
