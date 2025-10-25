use clap_sys::clap_window_t;

/// Represents a platform-specific GUI handle.
#[derive(Clone, Copy, Default)]
pub struct GuiHandle {
    pub window: clap_window_t,
}

/// Describes a request to attach a plug-in UI.
#[derive(Clone)]
pub struct GuiAttachRequest {
    pub api: &'static str,
    pub handle: GuiHandle,
}
