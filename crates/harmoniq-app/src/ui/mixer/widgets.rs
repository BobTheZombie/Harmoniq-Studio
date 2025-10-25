use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Rounding};

pub fn fill_panel(painter: &Painter, rect: Rect, color: Color32, border: Color32) {
    painter.rect(
        rect,
        Rounding::same(6.0),
        color,
        egui::Stroke::new(1.0, border),
    );
}

pub fn draw_label(painter: &Painter, rect: Rect, text: &str, color: Color32) {
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.0),
        color,
    );
}

pub fn draw_meter_bar(painter: &Painter, rect: Rect, amount: f32, fill: Color32, bg: Color32) {
    let amount = amount.clamp(0.0, 1.0);
    painter.rect_filled(rect, Rounding::same(2.0), bg);
    if amount <= 0.0 {
        return;
    }
    let height = rect.height() * amount;
    let fill_rect = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - height), rect.max);
    painter.rect_filled(fill_rect, Rounding::same(2.0), fill);
}
