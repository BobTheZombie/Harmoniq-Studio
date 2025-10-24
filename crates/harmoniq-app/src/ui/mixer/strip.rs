use eframe::egui::{
    self, align::Align2, pos2, Color32, FontId, Id, Layout, Painter, PointerButton, Pos2, Rect,
    Response, Sense, Shape, Stroke, Ui, Vec2,
};
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};

use crate::ui::mixer::context::strip_context_menu;
use crate::ui::mixer::inserts::InsertsView;
use crate::ui::mixer::layout::StripDensity;
use crate::ui::mixer::meter::{paint_meter, MeterState};
use crate::ui::mixer::sends::SendsView;
use crate::ui::mixer::theme::MixerTheme;

pub struct StripRenderArgs<'a> {
    pub ui: &'a mut Ui,
    pub api: &'a dyn MixerUiApi,
    pub info: &'a UiStripInfo,
    pub index: usize,
    pub density: StripDensity,
    pub theme: &'a MixerTheme,
    pub width: f32,
    pub height: f32,
    pub zoom: f32,
    pub is_selected: bool,
    pub meter: &'a mut MeterState,
    pub insert_labels: Vec<String>,
    pub send_labels: Vec<String>,
    pub group_highlight: bool,
}

pub struct StripResponse {
    pub clicked: bool,
    pub double_clicked: bool,
    pub context_requested: bool,
}

pub fn render_strip(args: StripRenderArgs<'_>) -> StripResponse {
    let StripRenderArgs {
        ui,
        api,
        info,
        index,
        density,
        theme,
        width,
        height,
        zoom,
        is_selected,
        meter,
        mut insert_labels,
        mut send_labels,
        group_highlight,
    } = args;

    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter_at(rect);

    let bg = tinted_strip_color(theme, info, is_selected, group_highlight);
    painter.rect(rect, theme.rounding_large, bg, theme.strip_border);

    let cap_height = 54.0 * zoom;
    let icon_row = 20.0 * zoom;
    let pan_section = 48.0 * zoom;
    let lists_height = height - cap_height - icon_row - pan_section - 160.0 * zoom;

    let mut inner = rect.shrink2(Vec2::new(6.0 * zoom, 8.0 * zoom));
    let top_cap_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), cap_height));
    draw_top_cap(&painter, top_cap_rect, info, theme, index);

    inner.min.y += cap_height + 4.0 * zoom;
    let icon_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), icon_row));
    let icon_resp = draw_icon_row(ui, icon_rect, api, info, index, theme);
    response = response.union(icon_resp);

    inner.min.y += icon_row + 6.0 * zoom;
    let pan_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), pan_section));
    let pan_resp = draw_pan_width(ui, pan_rect, api, info, index, theme, zoom);
    response = response.union(pan_resp);

    inner.min.y += pan_section + 6.0 * zoom;
    let inserts_height = lists_height * 0.55;
    let sends_height = lists_height * 0.45;

    let inserts_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), inserts_height));
    inner.min.y += inserts_height + 6.0 * zoom;
    let sends_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), sends_height));

    InsertsView::new(density, theme)
        .with_labels(&insert_labels)
        .show(ui, inserts_rect, api, info, index);
    SendsView::new(density, theme)
        .with_labels(&send_labels)
        .show(ui, sends_rect, api, info, index);

    inner.min.y += sends_height + 8.0 * zoom;

    let footer_height = 40.0 * zoom;
    let fader_height = rect.max.y - inner.min.y - footer_height - 6.0 * zoom;
    let fader_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), fader_height));
    inner.min.y += fader_height + 6.0 * zoom;
    let footer_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), footer_height));

    draw_fader_and_meter(ui, fader_rect, api, info, index, theme, meter, zoom);
    draw_footer(ui, footer_rect, api, info, index, theme);

    if let Some(inner_resp) = response.context_menu(|ui| {
        strip_context_menu(ui, api, index, info);
    }) {
        response = inner_resp.response;
    }

    StripResponse {
        clicked: response.clicked(),
        double_clicked: response.double_clicked(),
        context_requested: response.secondary_clicked(),
    }
}

fn tinted_strip_color(
    theme: &MixerTheme,
    info: &UiStripInfo,
    is_selected: bool,
    group_highlight: bool,
) -> Color32 {
    let mut bg = theme.strip_bg;
    if group_highlight {
        let tint = Color32::from_rgba_premultiplied(
            (info.color_rgba[0] * 255.0) as u8,
            (info.color_rgba[1] * 255.0) as u8,
            (info.color_rgba[2] * 255.0) as u8,
            60,
        );
        bg = bg.linear_interpolate(tint, 0.35);
    }
    if is_selected {
        bg = bg.linear_interpolate(theme.selection, 0.5);
    }
    bg
}

fn draw_top_cap(
    painter: &Painter,
    rect: Rect,
    info: &UiStripInfo,
    theme: &MixerTheme,
    index: usize,
) {
    let gradient_top = theme.cap_gradient_top;
    let gradient_bottom = theme.cap_gradient_bottom;

    painter.add(Shape::rect_filled(
        rect,
        theme.rounding_small,
        Color32::TRANSPARENT,
    ));
    let gradient_mesh = egui::epaint::Mesh::gradient_rectangle(rect, gradient_top, gradient_bottom);
    painter.add(Shape::mesh(gradient_mesh));

    let chip_rect = Rect::from_min_size(
        pos2(rect.min.x + 4.0, rect.min.y + 6.0),
        Vec2::new(18.0, 18.0),
    );
    painter.rect(
        chip_rect,
        theme.rounding_small,
        Color32::from_rgba_premultiplied(
            (info.color_rgba[0] * 255.0) as u8,
            (info.color_rgba[1] * 255.0) as u8,
            (info.color_rgba[2] * 255.0) as u8,
            (info.color_rgba[3] * 255.0) as u8,
        ),
        Stroke::NONE,
    );

    let index_text = format!("{:02}", index + 1);
    painter.text(
        pos2(chip_rect.max.x + 6.0, chip_rect.center().y),
        Align2::LEFT_CENTER,
        index_text,
        FontId::proportional(12.0),
        theme.header_text,
    );

    let name_rect = Rect::from_min_size(
        pos2(rect.min.x + 4.0, chip_rect.max.y + 6.0),
        Vec2::new(rect.width() - 8.0, 18.0),
    );
    let mut name = info.name.clone();
    if name.len() > 20 {
        name.truncate(20);
        name.push('…');
    }
    painter.text(
        name_rect.min,
        Align2::LEFT_TOP,
        name,
        FontId::proportional(12.0),
        theme.header_text,
    );

    let status = if info.pdc_active { "PDC" } else { "" };
    if !status.is_empty() {
        painter.text(
            pos2(rect.max.x - 6.0, chip_rect.center().y),
            Align2::RIGHT_CENTER,
            status,
            FontId::proportional(11.0),
            theme.header_text,
        );
    }
}

fn draw_icon_row(
    ui: &mut Ui,
    rect: Rect,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    index: usize,
    theme: &MixerTheme,
) -> Response {
    ui.allocate_ui_at_rect(rect, |ui| {
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            let arm = icon_button(ui, "⭘", info.armed, theme);
            if arm.clicked() {
                api.toggle_arm(index);
            }
            let mute = icon_button(ui, "M", info.muted, theme);
            if mute.clicked() {
                api.toggle_mute(index);
            }
            let solo = icon_button(ui, "S", info.soloed, theme);
            if solo.clicked() {
                api.toggle_solo(index);
            }
            let phase = icon_button(ui, "Ø", info.phase_invert, theme);
            if phase.clicked() {
                api.toggle_phase(index);
            }
            arm.union(mute).union(solo).union(phase)
        })
        .inner
    })
    .inner
}

fn icon_button(ui: &mut Ui, label: &str, active: bool, theme: &MixerTheme) -> Response {
    ui.add(
        egui::Button::new(label)
            .min_size(Vec2::new(22.0, 18.0))
            .rounding(theme.rounding_small)
            .fill(if active { theme.accent } else { theme.icon_bg }),
    )
}

fn draw_pan_width(
    ui: &mut Ui,
    rect: Rect,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    index: usize,
    theme: &MixerTheme,
    zoom: f32,
) -> Response {
    ui.allocate_ui_at_rect(rect, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            let mut pan = info.pan;
            let pan_resp = knob(ui, "PAN", &mut pan, -1.0..=1.0, 0.0, theme, zoom);
            if pan_resp.changed() {
                api.set_pan(index, pan);
            }
            if (pan_resp.double_clicked()
                || (pan_resp.clicked() && ui.ctx().input(|i| i.modifiers.alt)))
            {
                api.set_pan(index, 0.0);
            }

            let mut width = info.width;
            let width_resp = knob(ui, "WIDTH", &mut width, 0.0..=2.0, 1.0, theme, zoom);
            if width_resp.changed() {
                api.set_width(index, width);
            }
            if (width_resp.double_clicked()
                || (width_resp.clicked() && ui.ctx().input(|i| i.modifiers.alt)))
            {
                api.set_width(index, 1.0);
            }

            pan_resp.union(width_resp)
        })
        .inner
    })
    .inner
}

fn knob(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    theme: &MixerTheme,
    zoom: f32,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(44.0 * zoom), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let radius = rect.width().min(rect.height()) * 0.45;
    let center = rect.center();

    if response.dragged() {
        let speed = if ui.ctx().input(|i| i.modifiers.ctrl) {
            0.002
        } else {
            0.01
        };
        *value -= response.drag_delta().y * speed;
        *value = value.clamp(*range.start(), *range.end());
    }

    if response.clicked_by(PointerButton::Secondary) {
        *value = default;
    }

    let normalized = (*value - *range.start()) / (*range.end() - *range.start());
    let angle = std::f32::consts::PI * 1.5 * (normalized - 0.5);
    let indicator = Pos2::new(
        center.x + angle.sin() * radius,
        center.y - angle.cos() * radius,
    );

    painter.circle(
        center,
        radius,
        theme.knob_bg,
        Stroke::new(1.0, theme.strip_border.color),
    );
    painter.line_segment([center, indicator], Stroke::new(2.0, theme.accent));

    painter.text(
        pos2(center.x, rect.max.y + 4.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(10.0),
        theme.header_text,
    );

    response
}

fn draw_fader_and_meter(
    ui: &mut Ui,
    rect: Rect,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    index: usize,
    theme: &MixerTheme,
    meter: &mut MeterState,
    zoom: f32,
) {
    let meter_width = 18.0 * zoom;
    let fader_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - meter_width - 4.0, rect.max.y));
    let meter_rect = Rect::from_min_max(pos2(fader_rect.max.x + 4.0, rect.min.y), rect.max);

    let mut fader_db = info.fader_db;
    let response = draw_fader(ui, fader_rect, &mut fader_db, theme);
    if response.changed() {
        api.set_fader_db(index, fader_db);
    }
    if response.double_clicked() || (response.clicked() && ui.ctx().input(|i| i.modifiers.alt)) {
        api.set_fader_db(index, 0.0);
    }

    paint_meter(ui, meter_rect, Id::new(("meter", index)), meter, theme);
}

fn draw_fader(ui: &mut Ui, rect: Rect, value: &mut f32, theme: &MixerTheme) -> Response {
    ui.allocate_ui_at_rect(rect, |ui| {
        let (response_rect, response) =
            ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
        let painter = ui.painter_at(response_rect);

        let track = Rect::from_min_max(
            pos2(response_rect.center().x - 6.0, response_rect.min.y + 10.0),
            pos2(response_rect.center().x + 6.0, response_rect.max.y - 10.0),
        );
        painter.rect_filled(track, theme.rounding_small, theme.fader_track);

        draw_fader_ticks(&painter, track, theme);

        if response.dragged() {
            let speed = if ui.ctx().input(|i| i.modifiers.ctrl) {
                0.12
            } else {
                0.5
            };
            *value += response.drag_delta().y * -speed;
            *value = value.clamp(-90.0, 12.0);
        }

        let position = (*value + 90.0) / 102.0;
        let y = track.max.y - position * track.height();
        let knob_rect = Rect::from_center_size(pos2(track.center().x, y), Vec2::new(22.0, 12.0));
        painter.rect_filled(knob_rect, theme.rounding_small, theme.fader_thumb);
        painter.rect_stroke(
            knob_rect,
            theme.rounding_small,
            Stroke::new(1.0, theme.strip_border.color),
        );

        painter.text(
            pos2(track.max.x + 10.0, track.min.y - 12.0),
            Align2::RIGHT_CENTER,
            format!("{:+.1} dB", *value),
            FontId::proportional(11.0),
            theme.header_text,
        );

        response
    })
    .inner
}

fn draw_fader_ticks(painter: &Painter, track: Rect, theme: &MixerTheme) {
    const TICKS: [f32; 6] = [0.0, -6.0, -12.0, -18.0, -24.0, -90.0];
    for db in TICKS {
        let norm = (db + 90.0) / 102.0;
        let y = track.max.y - norm * track.height();
        let width = if (db - 0.0).abs() < f32::EPSILON {
            12.0
        } else {
            8.0
        };
        painter.line_segment(
            [pos2(track.min.x - width, y), pos2(track.min.x, y)],
            Stroke::new(1.0, theme.scale_tick),
        );
        let label = if db <= -80.0 {
            "-∞".to_string()
        } else {
            format!("{}", db as i32)
        };
        painter.text(
            pos2(track.min.x - width - 2.0, y),
            Align2::RIGHT_CENTER,
            label,
            FontId::proportional(9.0),
            theme.scale_text,
        );
    }
}

fn draw_footer(
    ui: &mut Ui,
    rect: Rect,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    index: usize,
    theme: &MixerTheme,
) {
    ui.allocate_ui_at_rect(rect, |ui| {
        let route = api.route_target_label(index);
        ui.label(
            egui::RichText::new(route)
                .color(theme.header_text)
                .size(11.0),
        );
        ui.label(
            egui::RichText::new(format!(
                "PDC {} | {:.1}%",
                samples_to_ms(info.latency_samples),
                info.cpu_percent
            ))
            .color(theme.header_text)
            .size(10.0),
        );
    });
}

fn samples_to_ms(samples: u32) -> String {
    if samples == 0 {
        "0 ms".to_string()
    } else {
        let ms = samples as f32 / 48_000.0 * 1000.0;
        format!("{ms:.1} ms")
    }
}
