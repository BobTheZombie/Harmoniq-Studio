//! Harmoniq channel rack facade.

pub mod convert;
pub mod state;
pub mod ui;

use state::{ChannelId, PatternId};

/// Callbacks supplied by the host app (harmoniq-app).
/// These are invoked from UI interactions; keep them non-RT.
pub struct RackCallbacks {
    /// Open the app's piano roll for (channel, pattern).
    pub open_piano_roll: Box<dyn FnMut(ChannelId, PatternId) + Send>,
    /// Show the plugin browser for adding/replacing an instrument on a channel.
    pub open_plugin_browser: Box<dyn FnMut(Option<ChannelId>) + Send>,
    /// Launch a file picker or accept a dropped path for a sample to add/replace on a channel.
    pub import_sample_file: Box<dyn FnMut(Option<ChannelId>, Option<std::path::PathBuf>) + Send>,
    /// Create an automation lane bound to a target parameter (by string key or id).
    pub create_automation_for: Box<dyn FnMut(&str) + Send>,
    /// Assign a mixer track for a rack channel.
    pub set_channel_mixer_track: Box<dyn FnMut(ChannelId, u16) + Send>,
}

impl RackCallbacks {
    /// Convenience constructor that sets all callbacks to no-ops.
    pub fn noop() -> Self {
        Self {
            open_piano_roll: Box::new(|_, _| {}),
            open_plugin_browser: Box::new(|_| {}),
            import_sample_file: Box::new(|_, _| {}),
            create_automation_for: Box::new(|_| {}),
            set_channel_mixer_track: Box::new(|_, _| {}),
        }
    }
}

pub struct RackProps<'a> {
    pub state: &'a mut state::RackState,
    pub callbacks: &'a mut RackCallbacks,
}

/// Render the Channel Rack widget (left/bottom pane in FL-like UX).
pub fn render(ui: &mut egui::Ui, props: RackProps<'_>) {
    ui::rack::render(ui, props);
}
