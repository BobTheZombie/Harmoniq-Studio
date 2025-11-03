use egui::{Color32, Stroke};

/// Visual design tokens used by the piano roll renderer.
#[derive(Clone, Debug)]
pub struct Theme {
    pub background: Color32,
    pub keyboard_background: Color32,
    pub keyboard_white: Color32,
    pub keyboard_black: Color32,
    pub keyboard_highlight: Color32,
    pub grid_bar: Stroke,
    pub grid_beat: Stroke,
    pub grid_subdivision: Stroke,
    pub grid_scale_highlight: Color32,
    pub grid_root_highlight: Color32,
    pub note_fill: Color32,
    pub note_border: Stroke,
    pub note_selected_fill: Color32,
    pub note_selected_border: Stroke,
    pub ghost_note_fill: Color32,
    pub ghost_note_border: Stroke,
    pub lane_background: Color32,
    pub lane_border: Stroke,
    pub text: Color32,
    pub ruler_background: Color32,
    pub ruler_foreground: Color32,
    pub playhead: Stroke,
    pub loop_range: Color32,
    pub selection_rect: Color32,
    pub selection_rect_border: Stroke,
}

impl Theme {
    /// Returns the default dark theme inspired by modern DAWs.
    pub fn dark() -> Self {
        Self {
            background: Color32::from_rgb(20, 21, 24),
            keyboard_background: Color32::from_rgb(32, 33, 38),
            keyboard_white: Color32::from_rgb(207, 209, 213),
            keyboard_black: Color32::from_rgb(60, 61, 65),
            keyboard_highlight: Color32::from_rgb(120, 170, 250),
            grid_bar: Stroke::new(2.0, Color32::from_rgb(60, 63, 70)),
            grid_beat: Stroke::new(1.0, Color32::from_rgb(50, 52, 58)),
            grid_subdivision: Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 62, 68, 120)),
            grid_scale_highlight: Color32::from_rgba_unmultiplied(60, 62, 70, 140),
            grid_root_highlight: Color32::from_rgb(70, 100, 150),
            note_fill: Color32::from_rgb(80, 180, 250),
            note_border: Stroke::new(1.0, Color32::from_rgb(120, 200, 255)),
            note_selected_fill: Color32::from_rgb(130, 220, 255),
            note_selected_border: Stroke::new(2.0, Color32::from_rgb(255, 255, 255)),
            ghost_note_fill: Color32::from_rgba_unmultiplied(90, 140, 250, 100),
            ghost_note_border: Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(120, 170, 255, 120),
            ),
            lane_background: Color32::from_rgb(30, 31, 35),
            lane_border: Stroke::new(1.0, Color32::from_rgb(50, 50, 54)),
            text: Color32::from_rgb(210, 214, 220),
            ruler_background: Color32::from_rgb(36, 37, 42),
            ruler_foreground: Color32::from_rgb(180, 182, 190),
            playhead: Stroke::new(2.0, Color32::from_rgb(255, 120, 60)),
            loop_range: Color32::from_rgba_unmultiplied(255, 160, 20, 64),
            selection_rect: Color32::from_rgba_unmultiplied(90, 160, 255, 40),
            selection_rect_border: Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(120, 180, 255, 200),
            ),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Helper describing spacing constants for the editor.
#[derive(Clone, Copy, Debug, Default)]
pub struct Spacing {
    pub keyboard_width: f32,
    pub lane_handle_width: f32,
    pub lane_spacing: f32,
    pub row_height_min: f32,
}

impl Spacing {
    pub fn compact() -> Self {
        Self {
            keyboard_width: 84.0,
            lane_handle_width: 18.0,
            lane_spacing: 6.0,
            row_height_min: 12.0,
        }
    }
}

/// Grid configuration shared between the editor and controller lanes.
#[derive(Clone, Debug)]
pub struct GridAppearance {
    pub show_triplets: bool,
    pub show_subdivisions: bool,
    pub scale_highlight_alpha: f32,
}

impl Default for GridAppearance {
    fn default() -> Self {
        Self {
            show_triplets: false,
            show_subdivisions: true,
            scale_highlight_alpha: 0.18,
        }
    }
}
