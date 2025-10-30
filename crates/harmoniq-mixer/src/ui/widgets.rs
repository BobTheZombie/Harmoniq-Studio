use egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

pub fn meter_vertical(
    ui: &mut egui::Ui,
    peak: f32,
    rms: f32,
    peak_hold: f32,
    clip: bool,
    size: Vec2,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect);
    let bg = ui.visuals().extreme_bg_color;
    p.rect_filled(rect, 2.0, bg);
    // draw RMS bar
    let rms_h = rect.height() * rms.clamp(0.0, 1.0);
    let rms_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.bottom() - rms_h),
        Vec2::new(rect.width(), rms_h),
    );
    p.rect_filled(rms_rect, 2.0, Color32::from_rgb(80, 180, 255));
    // draw Peak bar
    let pk_h = rect.height() * peak.clamp(0.0, 1.0);
    let pk_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.bottom() - pk_h),
        Vec2::new(rect.width(), pk_h),
    );
    p.rect_filled(pk_rect, 2.0, Color32::from_rgb(220, 90, 90));
    // peak hold line
    let hold_y = rect.bottom() - rect.height() * peak_hold.clamp(0.0, 1.0);
    p.line_segment(
        [
            Pos2::new(rect.left(), hold_y),
            Pos2::new(rect.right(), hold_y),
        ],
        Stroke::new(1.0, Color32::WHITE),
    );
    let led_radius = 3.5;
    let led_center = Pos2::new(rect.center().x, rect.top() + led_radius + 2.0);
    let led_color = if clip {
        Color32::from_rgb(250, 60, 60)
    } else {
        Color32::from_rgb(80, 80, 80)
    };
    p.circle_filled(led_center, led_radius, led_color);
    resp
}

pub fn small_knob(
    ui: &mut egui::Ui,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    text: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), Sense::click_and_drag());
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.45;
    let angle = egui::remap_clamp(*v, range.clone(), -2.5..=2.5);
    let changed = resp.dragged() || resp.clicked();
    if resp.dragged() {
        let delta = ui.input(|i| i.pointer.delta().y);
        let step = (*range.end() - *range.start()) * -delta * 0.003;
        *v = (*v + step).clamp(*range.start(), *range.end());
    }
    let p = ui.painter_at(rect);
    p.circle_filled(center, radius, ui.visuals().faint_bg_color);
    let end = egui::pos2(
        center.x + angle.cos() * radius * 0.8,
        center.y + angle.sin() * radius * 0.8,
    );
    p.line_segment([center, end], Stroke::new(2.0, ui.visuals().text_color()));
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::TextStyle::Small.resolve(ui.style()),
        ui.visuals().weak_text_color(),
    );
    p.galley(
        egui::pos2(rect.center().x - galley.size().x * 0.5, rect.bottom() + 2.0),
        galley,
        ui.visuals().text_color(),
    );
    changed
}

pub fn fader_db(
    ui: &mut egui::Ui,
    db: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    height: f32,
) -> bool {
    let width = 28.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    // background
    p.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    // value 0..1
    let t = (*db - *range.start()) / (*range.end() - *range.start());
    let y = egui::lerp(rect.bottom_up_range(), t);
    let handle = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 2.0, y - 6.0),
        egui::vec2(width - 4.0, 12.0),
    );
    p.rect_filled(handle, 3.0, ui.visuals().widgets.active.bg_fill);
    // lines
    for mark in [-60.0, -30.0, -18.0, -12.0, -6.0, 0.0, 6.0, 12.0] {
        if mark < *range.start() || mark > *range.end() {
            continue;
        }
        let mt = (mark - *range.start()) / (*range.end() - *range.start());
        let my = egui::lerp(rect.bottom_up_range(), mt);
        p.line_segment(
            [
                egui::pos2(rect.left(), my),
                egui::pos2(rect.left() + 8.0, my),
            ],
            Stroke::new(1.0, ui.visuals().weak_text_color()),
        );
    }
    if resp.dragged() {
        let dy = ui.input(|i| i.pointer.delta().y);
        let step = (*range.end() - *range.start()) * -dy * 0.004;
        *db = (*db + step).clamp(*range.start(), *range.end());
        return true;
    }
    if resp.double_clicked() {
        *db = 0.0;
        return true;
    }
    false
}
