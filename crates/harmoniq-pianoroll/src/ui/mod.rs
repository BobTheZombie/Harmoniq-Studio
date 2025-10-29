use egui::{self, Color32, Stroke};

pub mod pianoroll;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    N1_1,
    N1_2,
    N1_4,
    N1_8,
    N1_16,
    N1_32,
    N1_64,
    T1_8,
    T1_16,
    T1_32,
}

impl Snap {
    pub fn ticks(self, ppq: u32) -> u32 {
        let quarter = ppq;
        match self {
            Snap::N1_1 => 4 * quarter,
            Snap::N1_2 => 2 * quarter,
            Snap::N1_4 => quarter,
            Snap::N1_8 => quarter / 2,
            Snap::N1_16 => quarter / 4,
            Snap::N1_32 => quarter / 8,
            Snap::N1_64 => quarter / 16,
            Snap::T1_8 => (quarter as f32 * (2.0 / 3.0)) as u32,
            Snap::T1_16 => (quarter as f32 * (1.0 / 3.0)) as u32,
            Snap::T1_32 => (quarter as f32 * (1.0 / 6.0)) as u32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScaleGuide {
    None,
    Major(i8),
    Minor(i8),
}

pub fn is_black_key(key: i8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

pub fn lane_color(ui: &egui::Ui, key: i8) -> Color32 {
    let visuals = ui.visuals();
    if is_black_key(key) {
        visuals.extreme_bg_color.gamma_multiply(0.9)
    } else {
        visuals.faint_bg_color
    }
}

pub fn grid_stroke(ui: &egui::Ui, strong: bool) -> Stroke {
    if strong {
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color)
    } else {
        Stroke::new(1.0, ui.visuals().weak_text_color())
    }
}
