use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{Context, ViewportId};
use parking_lot::Mutex;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// Commands emitted by the host UI toward the plugin editor.
#[derive(Debug, Clone)]
pub enum EditorCommand {
    Close,
    RequestResize { width: u32, height: u32 },
    RequestFocus,
}

/// Events reported by the plugin editor back toward the UI host.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    Closed,
    Resized { width: u32, height: u32 },
    GainedFocus,
    LostFocus,
}

/// Handle that represents either a native OS window or an egui embedded
/// surface. UI code can use the handle to interact with the editor in a
/// platform-agnostic way.
#[derive(Debug, Clone)]
pub struct PluginEditorHandle {
    pub(crate) plugin_id: crate::PluginId,
    pub(crate) kind: PluginEditorKind,
}

#[derive(Debug, Clone)]
pub enum PluginEditorKind {
    Native(NativeEditorHandle),
    Egui(EguiEditorHandle),
}

/// Native editor hosting through raw-window-handle.
#[derive(Debug, Clone)]
pub struct NativeEditorHandle {
    pub window: RawWindowHandle,
    pub display: Option<RawDisplayHandle>,
    pub size: [u32; 2],
    pub commands: Sender<EditorCommand>,
    pub events: SharedReceiver<EditorEvent>,
}

/// Editor embedded within an egui viewport.
#[derive(Debug, Clone)]
pub struct EguiEditorHandle {
    pub context: Arc<Context>,
    pub viewport: ViewportId,
    pub commands: Sender<EditorCommand>,
    pub events: SharedReceiver<EditorEvent>,
}

#[derive(Debug, Clone)]
pub struct SharedReceiver<T> {
    inner: Arc<Mutex<Receiver<T>>>,
}

impl<T> SharedReceiver<T> {
    pub fn new(rx: Receiver<T>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(rx)),
        }
    }

    pub fn try_recv(&self) -> Option<T> {
        self.inner.lock().try_recv().ok()
    }

    pub fn drain(&self) -> Vec<T> {
        let mut guard = self.inner.lock();
        let mut events = Vec::new();
        while let Ok(event) = guard.try_recv() {
            events.push(event);
        }
        events
    }
}

impl PluginEditorHandle {
    pub fn plugin_id(&self) -> crate::PluginId {
        self.plugin_id
    }

    pub fn kind(&self) -> &PluginEditorKind {
        &self.kind
    }
}

pub(crate) fn create_egui_handle(
    plugin_id: crate::PluginId,
    ctx: Arc<Context>,
) -> (
    PluginEditorHandle,
    Receiver<EditorCommand>,
    Sender<EditorEvent>,
) {
    let (command_tx, command_rx) = unbounded();
    let (event_tx, event_rx) = unbounded();
    let viewport = ViewportId::from_hash_of(("harmoniq-plugin", plugin_id.0));

    let handle = PluginEditorHandle {
        plugin_id,
        kind: PluginEditorKind::Egui(EguiEditorHandle {
            context: ctx,
            viewport,
            commands: command_tx.clone(),
            events: SharedReceiver::new(event_rx),
        }),
    };

    (handle, command_rx, event_tx)
}

pub(crate) fn create_native_handle(
    plugin_id: crate::PluginId,
    window: RawWindowHandle,
    display: Option<RawDisplayHandle>,
    size: [u32; 2],
) -> (
    PluginEditorHandle,
    Receiver<EditorCommand>,
    Sender<EditorEvent>,
) {
    let (command_tx, command_rx) = unbounded();
    let (event_tx, event_rx) = unbounded();

    let handle = PluginEditorHandle {
        plugin_id,
        kind: PluginEditorKind::Native(NativeEditorHandle {
            window,
            display,
            size,
            commands: command_tx.clone(),
            events: SharedReceiver::new(event_rx),
        }),
    };

    (handle, command_rx, event_tx)
}
