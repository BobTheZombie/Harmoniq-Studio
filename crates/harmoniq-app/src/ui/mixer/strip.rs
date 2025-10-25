use eframe::egui::{self, Id, Painter, Pos2, Rect, Sense};
use harmoniq_engine::mixer::api::MixerUiApi;

use super::meter;
use super::render::MixerUiState;
use super::theme;
use super::widgets;

pub fn draw(
    painter_bg: &Painter,
    painter_mid: &Painter,
    painter_fg: &Painter,
    rect: Rect,
    idx: usize,
    _state: &MixerUiState,
    api: &dyn MixerUiApi,
) {
    let info = api.strip_info(idx);
    let theme = theme::active_theme();

    widgets::fill_panel(painter_bg, rect, theme.strip_bg, theme.strip_border);

    let id = Id::new(("mixer", "strip", idx));
    let response = painter_fg.ctx().interact(rect, id, Sense::click());
    if response.hovered() {
        painter_fg.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, theme.overlay));
    }

    let header_height = rect.height() * 0.18;
    let header_rect =
        Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + header_height));
    widgets::draw_label(
        painter_fg,
        header_rect,
        info.name.as_str(),
        theme.text_primary,
    );

    let meter_width = rect.width() * 0.25;
    let meter_rect = Rect::from_min_max(
        Pos2::new(rect.center().x - meter_width * 0.5, header_rect.max.y + 8.0),
        Pos2::new(rect.center().x + meter_width * 0.5, rect.max.y - 32.0),
    );
    let levels = api.level_fetch(idx);
    let peak = levels.0.max(levels.1);
    let amount = meter::level_to_amount(peak);
    widgets::draw_meter_bar(
        painter_mid,
        meter_rect,
        amount,
        theme.meter_fill,
        theme.meter_bg,
    );

    let footer_rect = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 24.0), rect.max);
    let db_text = format!("{:.1} dB", info.fader_db);
    painter_fg.text(
        footer_rect.center(),
        egui::Align2::CENTER_CENTER,
        db_text,
        egui::FontId::proportional(12.0),
        theme.text_primary,
    );
}

pub fn draw_master(painter: &Painter, rect: Rect, idx: usize, api: &dyn MixerUiApi) {
    let theme = theme::active_theme();
    painter.rect(
        rect,
        egui::Rounding::same(8.0),
        theme.master_bg,
        egui::Stroke::new(1.0, theme.strip_border),
    );

    let info = api.strip_info(idx);
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 18.0),
        egui::Align2::CENTER_CENTER,
        info.name.as_str(),
        egui::FontId::proportional(14.0),
        theme.text_primary,
    );

    let levels = api.level_fetch(idx);
    let amount = meter::level_to_amount(levels.0.max(levels.1));
    let meter_rect = Rect::from_min_max(
        Pos2::new(rect.center().x - rect.width() * 0.2, rect.min.y + 36.0),
        Pos2::new(rect.center().x + rect.width() * 0.2, rect.max.y - 40.0),
    );
    widgets::draw_meter_bar(
        painter,
        meter_rect,
        amount,
        theme.meter_fill,
        theme.meter_bg,
    );
}
