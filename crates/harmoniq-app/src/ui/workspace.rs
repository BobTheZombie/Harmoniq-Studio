use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspacePane {
    Browser,
    Arrange,
    Mixer,
    PianoRoll,
    Inspector,
    Console,
}

impl WorkspacePane {
    pub fn title(&self) -> &'static str {
        match self {
            WorkspacePane::Browser => "Browser",
            WorkspacePane::Arrange => "Arrange",
            WorkspacePane::Mixer => "Mixer",
            WorkspacePane::PianoRoll => "Piano Roll",
            WorkspacePane::Inspector => "Inspector",
            WorkspacePane::Console => "Console",
        }
    }
}

pub fn build_default_workspace() -> DockState<WorkspacePane> {
    let mut dock = DockState::new(vec![WorkspacePane::Arrange]);
    {
        let surface = dock.main_surface_mut();
        let [arrange_node, _browser] =
            surface.split_left(NodeIndex::root(), 0.8, vec![WorkspacePane::Browser]);
        let [arrange_node, inspector_node] =
            surface.split_right(arrange_node, 0.8, vec![WorkspacePane::Inspector]);
        let [_inspector, _console] =
            surface.split_below(inspector_node, 0.55, vec![WorkspacePane::Console]);
        let [arrange_node, piano_node] =
            surface.split_below(arrange_node, 0.62, vec![WorkspacePane::PianoRoll]);
        let [_piano, _mixer] = surface.split_below(piano_node, 0.58, vec![WorkspacePane::Mixer]);
    }
    dock
}
