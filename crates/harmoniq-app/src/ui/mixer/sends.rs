use eframe::egui::{self, pos2, Align2, Id, Layout, Rect, Sense, Stroke, Ui, Vec2};
use harmoniq_engine::mixer::api::MixerUiApi;

use crate::ui::mixer::layout::StripDensity;
use crate::ui::mixer::theme::MixerTheme;

pub struct SendsView<'a> {
    density: StripDensity,
    theme: &'a MixerTheme,
    labels: &'a [String],
}

impl<'a> SendsView<'a> {
    pub fn new(density: StripDensity, theme: &'a MixerTheme) -> Self {
        Self {
            density,
            theme,
            labels: &[],
        }
    }

    pub fn with_labels(mut self, labels: &'a [String]) -> Self {
        self.labels = labels;
        self
    }

    pub fn show(
        self,
        ui: &mut Ui,
        rect: Rect,
        api: &dyn MixerUiApi,
        info: &UiStripInfo,
        strip_index: usize,
    ) {
        let visible = match self.density {
            StripDensity::Narrow => 4,
            StripDensity::Wide => 6,
        };
        let total = info.send_count;
        let row_height = 22.0;

        ui.allocate_ui_at_rect(rect, |ui| {
            ui.set_min_size(Vec2::new(rect.width(), visible as f32 * row_height));
            ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
                for slot in 0..visible.min(total) {
                    let label = self.labels.get(slot).map(|s| s.as_str()).unwrap_or("");
                    draw_send_row(ui, api, strip_index, slot, label, self.theme);
                }
            });
        });

        if total > visible {
            let button_rect = Rect::from_min_size(
                pos2(rect.max.x - 22.0, rect.max.y - 18.0),
                Vec2::new(18.0, 18.0),
            );
            let popup_id = Id::new(("send_overflow", info.id));
            let response = ui.put(button_rect, egui::Button::new("⋯"));
            if response.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            egui::popup::popup_below_widget(ui, popup_id, &response, |ui| {
                ui.set_min_width(200.0);
                ui.heading("Sends");
                ui.separator();
                for slot in 0..total {
                    let label = self.labels.get(slot).map(|s| s.as_str()).unwrap_or("");
                    draw_send_row(ui, api, strip_index, slot, label, self.theme);
                }
            });
        }
    }
}

fn draw_send_row(
    ui: &mut Ui,
    api: &dyn MixerUiApi,
    strip_index: usize,
    slot: usize,
    label: &str,
    theme: &MixerTheme,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 22.0),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        theme.rounding_small,
        theme.inactive_slot,
        Stroke::new(1.0, theme.slot_border.color),
    );

    painter.text(
        pos2(rect.min.x + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{:02}. {}", slot + 1, label),
        egui::FontId::proportional(11.0),
        theme.header_text,
    );

    let mut level = api.send_level(strip_index, slot);
    let slider_width = (rect.width() * 0.45).clamp(60.0, 110.0);
    let slider_rect = Rect::from_min_size(
        pos2(rect.min.x + 96.0, rect.min.y + 4.0),
        Vec2::new(slider_width, 14.0),
    );
    let slider = ui.put(
        slider_rect,
        egui::Slider::new(&mut level, -60.0..=6.0)
            .show_value(false)
            .small(true),
    );
    if slider.changed() {
        api.send_set_level(strip_index, slot, level);
    }

    let readout = format!("{level:+.1} dB");
    painter.text(
        pos2(slider_rect.max.x + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        readout,
        egui::FontId::proportional(10.0),
        theme.header_text,
    );

    let pre = api.send_is_pre(strip_index, slot);
    let toggle_rect = Rect::from_min_size(
        pos2(rect.max.x - 48.0, rect.min.y + 2.0),
        Vec2::new(40.0, 18.0),
    );
    let toggle = ui.put(
        toggle_rect,
        egui::SelectableLabel::new(pre, if pre { "PRE" } else { "POST" }).text_color(if pre {
            theme.accent
        } else {
            theme.header_text
        }),
    );
    if toggle.clicked() {
        api.send_toggle_pre(strip_index, slot);
    }
}
