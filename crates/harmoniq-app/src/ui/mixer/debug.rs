use eframe::egui::{self, pos2, Color32, FontId, Rect, Stroke, Vec2};

use super::layout::{self, Layout};
use super::render::MixerUiState;

pub fn overlay(
    ui: &egui::Ui,
    layout: &Layout,
    state: &MixerUiState,
    first: usize,
    last: usize,
    viewport: Rect,
) {
    let painter = ui.painter();
    let color = Color32::from_rgba_unmultiplied(255, 255, 255, 90);
    let stroke = Stroke::new(1.0, color);

    for idx in first..last {
        let x = layout::world_x(layout, idx) - state.scroll_x;
        let snapped = layout::snap_px(ui.ctx(), viewport.min.x + x);
        let start = pos2(snapped, viewport.min.y);
        let end = pos2(snapped, viewport.max.y);
        painter.line_segment([start, end], stroke);
        painter.text(
            start + Vec2::new(4.0, 4.0),
            egui::Align2::LEFT_TOP,
            format!("#{idx}"),
            FontId::monospace(10.0),
            color,
        );
    }

    let header = format!(
        "scroll_x={:.1} zoom={:.2} strip_w={:.1} gap={:.1} ppp={:.2}",
        state.scroll_x,
        layout.zoom,
        layout.strip_w_pt,
        layout.gap_pt,
        ui.ctx().pixels_per_point(),
    );
    painter.text(
        viewport.min + Vec2::new(8.0, viewport.height() - 16.0),
        egui::Align2::LEFT_BOTTOM,
        header,
        FontId::monospace(11.0),
        color,
    );
}
