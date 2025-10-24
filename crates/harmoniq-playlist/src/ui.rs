//! Placeholder UI helpers for the playlist module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistViewState {
    pub zoom: f32,
    pub scroll_offset: f32,
    pub selection: Vec<u32>,
}

impl PlaylistViewState {
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.25, 4.0);
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_clamped() {
        let mut state = PlaylistViewState::default();
        state.set_zoom(10.0);
        assert!((state.zoom - 4.0).abs() < f32::EPSILON);
        state.set_zoom(0.1);
        assert!((state.zoom - 0.25).abs() < f32::EPSILON);
    }
}
