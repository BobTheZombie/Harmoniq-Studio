//! Harmoniq channel rack facade.

pub mod commands;
pub mod convert;
pub mod state;
pub mod ui;

use state::{ChannelId, PatternId};

pub struct RackCallbacks {
    pub open_piano_roll: Box<dyn FnMut(ChannelId, PatternId) + Send + 'static>,
    pub open_plugin_browser: Box<dyn FnMut(ChannelId) + Send + 'static>,
    pub import_sample_file: Box<dyn FnMut(ChannelId) + Send + 'static>,
}

impl Default for RackCallbacks {
    fn default() -> Self {
        Self {
            open_piano_roll: Box::new(|_, _| {}),
            open_plugin_browser: Box::new(|_| {}),
            import_sample_file: Box::new(|_| {}),
        }
    }
}

pub struct RackProps<'a> {
    pub state: &'a mut state::RackState,
    pub callbacks: &'a mut RackCallbacks,
}

pub fn render(ui: &mut egui::Ui, props: RackProps<'_>) {
    ui::rack_ui(ui, props);
}
