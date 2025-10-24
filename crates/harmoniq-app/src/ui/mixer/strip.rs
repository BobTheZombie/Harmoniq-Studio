use eframe::egui::{self, Align, Align2, Layout, RichText, Sense, Ui, Vec2};
use egui::epaint::Shadow;
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};

use crate::ui::mixer::context_menu::strip_context_menu;
use crate::ui::mixer::layout::StripDensity;
use crate::ui::mixer::meter::{paint_meter, MeterState};
use crate::ui::mixer::slots::{overflow_button, SlotKind, SlotView};
use crate::ui::mixer::theme::MixerTheme;
use crate::ui::mixer::{gain_db_to_slider, slider_to_gain_db};

pub struct StripRenderArgs<'a> {
    pub ui: &'a mut Ui,
    pub api: &'a dyn MixerUiApi,
    pub info: &'a UiStripInfo,
    pub index: usize,
    pub density: StripDensity,
    pub theme: &'a MixerTheme,
    pub width: f32,
    pub height: f32,
    pub is_selected: bool,
    pub meter: &'a mut MeterState,
    pub insert_labels: Vec<String>,
    pub send_labels: Vec<String>,
}

pub struct StripResponse {
    pub clicked: bool,
    pub double_clicked: bool,
    pub context_requested: bool,
}

pub fn render_strip(mut args: StripRenderArgs<'_>) -> StripResponse {
    let StripRenderArgs {
        ui,
        api,
        info,
        density,
        theme,
        width,
        height,
        is_selected,
        meter,
        insert_labels,
        send_labels,
    } = args;

    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter_at(rect);

    let mut fill = theme.strip_bg;
    if is_selected {
        fill = theme.strip_bg.linear_multiply(1.2);
    }
    painter.rect(rect, theme.rounding_large, fill, Shadow::NONE);

    if response.hovered() {
        painter.rect_stroke(rect, theme.rounding_large, theme.strip_border);
    }

    let inner = rect.shrink2(Vec2::new(6.0, 8.0));
    let mut child = ui.child_ui(inner, Layout::top_down(Align::Center));

    render_header(&mut child, info, density, theme);

    child.add_space(4.0);
    render_status_icons(&mut child, api, info, theme, index);

    child.add_space(6.0);
    render_fader(&mut child, api, info, theme, index);
    child.add_space(4.0);

    render_pan_and_width(&mut child, api, info, theme, index);

    child.add_space(6.0);
    render_inserts(&mut child, api, info, density, theme, insert_labels, index);
    child.add_space(6.0);
    render_sends(&mut child, api, info, density, theme, send_labels, index);

    child.add_space(6.0);
    render_meter(&mut child, meter, theme, height * 0.35);

    response = response.context_menu(|ui| {
        strip_context_menu(ui);
    });

    StripResponse {
        clicked: response.clicked(),
        double_clicked: response.double_clicked(),
        context_requested: response.secondary_clicked(),
    }
}

fn render_header(ui: &mut Ui, info: &UiStripInfo, density: StripDensity, theme: &MixerTheme) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 24.0),
        Layout::left_to_right(),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            let (color_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
            ui.painter().rect_filled(
                color_rect,
                theme.rounding_small,
                egui::Color32::from_rgba_premultiplied(
                    (info.color_rgba[0] * 255.0) as u8,
                    (info.color_rgba[1] * 255.0) as u8,
                    (info.color_rgba[2] * 255.0) as u8,
                    (info.color_rgba[3] * 255.0) as u8,
                ),
            );

            let name = if density == StripDensity::Narrow {
                truncate_vertical(&info.name)
            } else {
                info.name.clone()
            };

            if density == StripDensity::Narrow {
                let (name_rect, _) = ui.allocate_exact_size(Vec2::new(18.0, 48.0), Sense::hover());
                let painter = ui.painter();
                painter.text(
                    name_rect.center(),
                    Align2::CENTER_CENTER,
                    name,
                    egui::TextStyle::Body.resolve(ui.style()),
                    theme.header_text,
                );
            } else {
                ui.label(RichText::new(name).color(theme.header_text).strong());
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let latency_text =
                    format!("{:.1} ms", info.latency_samples as f32 / 48_000.0 * 1000.0);
                ui.label(
                    RichText::new(latency_text)
                        .size(11.0)
                        .color(theme.header_text),
                );
            });
        },
    );
}

fn render_status_icons(
    ui: &mut Ui,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    theme: &MixerTheme,
    index: usize,
) {
    ui.horizontal(|ui| {
        if toggle_button(ui, "M", info.muted, theme) {
            api.toggle_mute(index);
        }
        if toggle_button(ui, "S", info.soloed, theme) {
            api.toggle_solo(index);
        }
        if toggle_button(ui, "R", info.armed, theme) {
            api.toggle_arm(index);
        }
        if toggle_button(ui, "Ø", info.phase_invert, theme) {
            api.toggle_phase(index);
        }
    });
}

fn render_fader(
    ui: &mut Ui,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    _theme: &MixerTheme,
    index: usize,
) {
    let mut slider_value = gain_db_to_slider(info.gain_db).max(0.001);
    let slider = egui::Slider::new(&mut slider_value, 0.001..=1.0)
        .vertical()
        .logarithmic(true)
        .text("dB")
        .custom_formatter(|v, _| format!("{:.1} dB", slider_to_gain_db(v)))
        .custom_parser(|text| text.parse::<f32>().map(gain_db_to_slider).ok());
    let response = ui.add(slider);
    if response.changed() {
        api.set_gain_db(index, slider_to_gain_db(slider_value));
    }
    if response.double_clicked() {
        api.set_gain_db(index, 0.0);
    }
}

fn render_pan_and_width(
    ui: &mut Ui,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    _theme: &MixerTheme,
    index: usize,
) {
    ui.horizontal(|ui| {
        let mut pan = info.pan;
        if ui
            .add(
                egui::DragValue::new(&mut pan)
                    .speed(0.01)
                    .clamp_range(-1.0..=1.0)
                    .prefix("Pan "),
            )
            .changed()
        {
            api.set_pan(index, pan);
        }

        let mut width = info.width;
        if ui
            .add(
                egui::DragValue::new(&mut width)
                    .speed(0.01)
                    .clamp_range(0.0..=2.0)
                    .prefix("Width "),
            )
            .changed()
        {
            api.set_width(index, width);
        }
    });
}

fn render_inserts(
    ui: &mut Ui,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    density: StripDensity,
    theme: &MixerTheme,
    labels: Vec<String>,
    index: usize,
) {
    let visible = match density {
        StripDensity::Narrow => 6,
        StripDensity::Wide => 10,
    };
    let total = info.inserts;
    let mut grid = egui::Grid::new(("insert_grid", info.id)).spacing(Vec2::splat(4.0));
    grid.show(ui, |ui| {
        for idx in 0..visible.min(total) {
            SlotView {
                index: idx,
                label: labels.get(idx).map(|s| s.as_str()).unwrap_or(""),
                active: true,
                pre_fader: api.insert_is_pre(index, idx),
                kind: SlotKind::Insert,
            }
            .show(ui, theme);
            ui.end_row();
        }
    });

    if total > visible {
        overflow_button(ui, total, total - visible, theme);
    }
}

fn render_sends(
    ui: &mut Ui,
    api: &dyn MixerUiApi,
    info: &UiStripInfo,
    density: StripDensity,
    theme: &MixerTheme,
    labels: Vec<String>,
    index: usize,
) {
    let visible = match density {
        StripDensity::Narrow => 2,
        StripDensity::Wide => 4,
    };
    let total = info.sends;
    let mut grid = egui::Grid::new(("send_grid", info.id)).spacing(Vec2::splat(4.0));
    grid.show(ui, |ui| {
        for idx in 0..visible.min(total) {
            SlotView {
                index: idx,
                label: labels.get(idx).map(|s| s.as_str()).unwrap_or(""),
                active: true,
                pre_fader: api.send_is_pre(index, idx),
                kind: SlotKind::Send,
            }
            .show(ui, theme);
            ui.end_row();
        }
    });

    if total > visible {
        overflow_button(ui, total, total - visible, theme);
    }
}

fn render_meter(ui: &mut Ui, meter: &mut MeterState, theme: &MixerTheme, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    paint_meter(&painter, rect, meter, theme);
}

fn truncate_vertical(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    for ch in name.chars().take(10) {
        output.push(ch);
        output.push('\n');
    }
    output
}

fn toggle_button(ui: &mut Ui, label: &str, active: bool, theme: &MixerTheme) -> bool {
    let fill = if active {
        theme.accent
    } else {
        theme.inactive_slot
    };
    let resp = ui.add_sized(Vec2::new(20.0, 20.0), egui::Button::new(label).fill(fill));
    resp.clicked()
}
