use std::ops::RangeInclusive;

use egui::{self, pos2, vec2, Align2, Color32, Response, Sense, Ui, Widget};

use super::model::MixerTheme;
use super::rt_api::PanLaw;

pub struct GainFader<'a> {
    value_db: &'a mut f32,
    theme: MixerTheme,
}

impl<'a> GainFader<'a> {
    pub fn new(value_db: &'a mut f32, theme: MixerTheme) -> Self {
        Self { value_db, theme }
    }

    fn range(&self) -> RangeInclusive<f32> {
        self.theme.metrics.fader_range.clone()
    }
}

impl Widget for GainFader<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let range = self.range();
        let desired_size = vec2(28.0, 180.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        if response.double_clicked() {
            *self.value_db = 0.0;
            response.mark_changed();
        }

        if response.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta().y);
            if delta.abs() > f32::EPSILON {
                let mut value = *self.value_db - delta * 0.5;
                let min = *range.start();
                let max = *range.end();
                value = value.clamp(min, max);
                if (value - *self.value_db).abs() > f32::EPSILON {
                    *self.value_db = value;
                    response.mark_changed();
                }
            }
        }

        let painter = ui.painter_at(rect);
        let rounding = egui::Rounding::same(4.0);
        painter.rect_filled(
            rect,
            rounding,
            Color32::from_rgba_unmultiplied(20, 20, 24, 220),
        );
        painter.rect_stroke(
            rect,
            rounding,
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180)),
        );

        let min = *range.start();
        let max = *range.end();
        let norm = db_to_norm(*self.value_db, min, max);
        let fill_height = rect.height() * norm;
        let fill_rect = egui::Rect::from_min_size(
            pos2(rect.left() + 4.0, rect.bottom() - fill_height),
            vec2(rect.width() - 8.0, fill_height),
        );
        let gradient_color: Color32 = egui::epaint::Hsva::new(0.36, 0.85, 0.75, 1.0).into();
        painter.rect_filled(fill_rect, egui::Rounding::same(3.0), gradient_color);

        let handle_y = rect.bottom() - fill_height;
        let handle_rect = egui::Rect::from_center_size(
            pos2(rect.center().x, handle_y),
            vec2(rect.width() - 6.0, 6.0),
        );
        painter.rect_filled(
            handle_rect,
            egui::Rounding::same(2.0),
            Color32::from_rgb(240, 240, 255),
        );
        painter.rect_stroke(
            handle_rect,
            egui::Rounding::same(2.0),
            egui::Stroke::new(1.0, Color32::from_rgb(0, 0, 0)),
        );

        painter.text(
            pos2(rect.center().x, rect.bottom() + 4.0),
            Align2::CENTER_TOP,
            format!("{:+.1} dB", *self.value_db),
            egui::TextStyle::Small.resolve(ui.style()),
            Color32::from_rgb(220, 220, 230),
        );

        response
    }
}

fn db_to_norm(db: f32, min: f32, max: f32) -> f32 {
    let db = db.clamp(min, max);
    let linear = 10.0_f32.powf(db / 20.0);
    let min_lin = 10.0_f32.powf(min / 20.0);
    let max_lin = 10.0_f32.powf(max / 20.0);
    ((linear - min_lin) / (max_lin - min_lin)).clamp(0.0, 1.0)
}

pub struct PanKnob<'a> {
    value: &'a mut f32,
    pan_law: PanLaw,
}

impl<'a> PanKnob<'a> {
    pub fn new(value: &'a mut f32, pan_law: PanLaw) -> Self {
        Self { value, pan_law }
    }
}

impl Widget for PanKnob<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired = vec2(42.0, 42.0);
        let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        if response.double_clicked() {
            *self.value = 0.0;
            response.mark_changed();
        }
        if response.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta());
            let mut value = *self.value + (delta.x - delta.y) * 0.01;
            value = value.clamp(-1.0, 1.0);
            if (value - *self.value).abs() > f32::EPSILON {
                *self.value = value;
                response.mark_changed();
            }
        }

        let painter = ui.painter_at(rect);
        painter.circle_filled(
            rect.center(),
            rect.width() * 0.5,
            Color32::from_rgb(32, 32, 40),
        );
        painter.circle_stroke(
            rect.center(),
            rect.width() * 0.5,
            egui::Stroke::new(1.0, Color32::from_rgb(0, 0, 0)),
        );

        let angle = (*self.value * std::f32::consts::FRAC_PI_2) - std::f32::consts::FRAC_PI_2;
        let pointer_len = rect.width() * 0.4;
        let dir = egui::Vec2::angled(angle);
        let pointer = pos2(
            rect.center().x + dir.x * pointer_len,
            rect.center().y + dir.y * pointer_len,
        );
        painter.line_segment(
            [rect.center(), pointer],
            egui::Stroke::new(2.5, Color32::from_rgb(240, 240, 255)),
        );

        let (left_db, right_db) = pan_gains_db(*self.value, self.pan_law);
        if response.hovered() {
            response =
                response.on_hover_text(format!("L: {:+.1} dB\nR: {:+.1} dB", left_db, right_db));
        }
        response
    }
}

pub struct MeterDisplay {
    peak_l: f32,
    peak_r: f32,
    rms_l: f32,
    rms_r: f32,
    theme: MixerTheme,
}

impl MeterDisplay {
    pub fn new(peak_l: f32, peak_r: f32, rms_l: f32, rms_r: f32, theme: MixerTheme) -> Self {
        Self {
            peak_l,
            peak_r,
            rms_l,
            rms_r,
            theme,
        }
    }
}

impl Widget for MeterDisplay {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired = vec2(24.0, 160.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            egui::Rounding::same(3.0),
            Color32::from_rgb(18, 20, 24),
        );
        painter.rect_stroke(
            rect,
            egui::Rounding::same(3.0),
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 220)),
        );

        draw_meter_bar(&painter, rect, self.peak_l, self.rms_l, true, &self.theme);
        draw_meter_bar(&painter, rect, self.peak_r, self.rms_r, false, &self.theme);

        response
    }
}

fn draw_meter_bar(
    painter: &egui::Painter,
    rect: egui::Rect,
    peak: f32,
    rms: f32,
    left: bool,
    theme: &MixerTheme,
) {
    let min_db = -60.0;
    let max_db = 6.0;
    let norm_peak = db_to_norm(peak, min_db, max_db);
    let norm_rms = db_to_norm(rms, min_db, max_db);

    let half_width = rect.width() * 0.5 - 2.0;
    let x = if left {
        rect.left() + 2.0
    } else {
        rect.center().x + 2.0
    };
    let meter_rect = egui::Rect::from_min_size(
        pos2(x, rect.bottom() - rect.height() * norm_peak),
        vec2(half_width, rect.height() * norm_peak),
    );
    painter.rect_filled(meter_rect, egui::Rounding::same(2.0), theme.meter_peak);

    let rms_height = rect.height() * norm_rms;
    let rms_rect =
        egui::Rect::from_min_size(pos2(x, rect.bottom() - rms_height), vec2(half_width, 2.0));
    painter.rect_filled(rms_rect, egui::Rounding::same(1.0), theme.meter_rms);

    let hold_y = rect.bottom() - rect.height() * norm_peak;
    let hold_rect = egui::Rect::from_min_size(pos2(x, hold_y), vec2(half_width, 1.5));
    painter.rect_filled(hold_rect, egui::Rounding::same(0.5), theme.meter_hold);
}

fn pan_gains_db(pan: f32, law: PanLaw) -> (f32, f32) {
    let (l, r) = pan_gains_linear(pan, law);
    (linear_to_db(l), linear_to_db(r))
}

fn pan_gains_linear(pan: f32, law: PanLaw) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    let pos = (pan + 1.0) * 0.5;
    let (mut left, mut right) = if pos <= 0.5 {
        (1.0, (pos * 2.0).sqrt())
    } else {
        (((1.0 - pos) * 2.0).sqrt(), 1.0)
    };

    let gain = match law {
        PanLaw::Linear => 1.0,
        PanLaw::Minus3dB => db_to_linear(-3.0),
        PanLaw::Minus4Point5dB => db_to_linear(-4.5),
    };
    left *= gain;
    right *= gain;
    (left, right)
}

fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn linear_to_db(value: f32) -> f32 {
    if value <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * value.log10()
    }
}
