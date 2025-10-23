use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspacePane {
    ChannelRack,
    PianoRoll,
    Mixer,
    Playlist,
}

impl WorkspacePane {
    pub fn title(&self) -> &'static str {
        match self {
            WorkspacePane::ChannelRack => "Channel Rack",
            WorkspacePane::PianoRoll => "Piano Roll",
            WorkspacePane::Mixer => "Mixer",
            WorkspacePane::Playlist => "Playlist",
        }
    }
}

pub fn build_default_workspace() -> DockState<WorkspacePane> {
    let mut dock = DockState::new(vec![WorkspacePane::Playlist]);
    {
        let surface = dock.main_surface_mut();
        let [playlist_node, channel_node] =
            surface.split_left(NodeIndex::root(), 0.75, vec![WorkspacePane::ChannelRack]);
        let [_channel, _piano] =
            surface.split_below(channel_node, 0.55, vec![WorkspacePane::PianoRoll]);
        let [_playlist, _mixer] =
            surface.split_below(playlist_node, 0.65, vec![WorkspacePane::Mixer]);
    }
    dock
}
