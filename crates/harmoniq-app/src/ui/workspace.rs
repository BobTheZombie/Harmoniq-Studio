use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspacePane {
    Browser,
    PianoRoll,
    Inspector,
    Console,
}

impl WorkspacePane {
    pub fn title(&self) -> &'static str {
        match self {
            WorkspacePane::Browser => "Browser",
            WorkspacePane::PianoRoll => "Piano Roll",
            WorkspacePane::Inspector => "Inspector",
            WorkspacePane::Console => "Console",
        }
    }
}

pub fn build_default_workspace() -> DockState<WorkspacePane> {
    let mut dock = DockState::new(vec![WorkspacePane::PianoRoll]);
    {
        let surface = dock.main_surface_mut();
        let [_browser_node, center_node] =
            surface.split_left(NodeIndex::root(), 0.26, vec![WorkspacePane::Browser]);
        let [center_node, inspector_node] =
            surface.split_right(center_node, 0.78, vec![WorkspacePane::Inspector]);
        let [_inspector, _console] =
            surface.split_below(inspector_node, 0.56, vec![WorkspacePane::Console]);
    }
    dock
}
