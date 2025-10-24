use eframe::egui::{self, pos2, Align2, Id, Layout, Rect, Response, Sense, Stroke, Ui, Vec2};
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};

use crate::ui::mixer::layout::StripDensity;
use crate::ui::mixer::theme::MixerTheme;

pub struct InsertsView<'a> {
    density: StripDensity,
    theme: &'a MixerTheme,
    labels: &'a [String],
}

impl<'a> InsertsView<'a> {
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
            StripDensity::Narrow => 8,
            StripDensity::Wide => 12,
        };
        let total = info.insert_count;
        let row_height = 20.0;

        let drag_id = Id::new(("insert_drag", info.id));
        let mut dragging = ui
            .ctx()
            .data_mut(|d| d.get_temp::<Option<usize>>(drag_id).unwrap_or(None));
        let mut drop_target: Option<usize> = None;

        ui.allocate_ui_at_rect(rect, |ui| {
            ui.set_min_size(Vec2::new(rect.width(), visible as f32 * row_height));
            ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
                for slot in 0..visible.min(total) {
                    let label = self.labels.get(slot).map(|s| s.as_str()).unwrap_or("");
                    let bypassed = api.insert_is_bypassed(strip_index, slot);
                    let row_response = draw_insert_row(
                        ui,
                        slot,
                        label,
                        bypassed,
                        self.theme,
                        dragging == Some(slot),
                        drop_target == Some(slot),
                    );

                    if row_response.clicked() {
                        api.insert_toggle_bypass(strip_index, slot);
                    }
                    if row_response.drag_started() {
                        dragging = Some(slot);
                        ui.ctx().request_repaint();
                    }
                    if dragging.is_some() && row_response.hovered() {
                        drop_target = Some(slot);
                    }
                    if row_response.dragged_stopped() {
                        drop_target = Some(slot);
                        ui.ctx().request_repaint();
                    }
                }
            });
        });

        if total > visible {
            let button_rect = Rect::from_min_size(
                pos2(rect.max.x - 22.0, rect.max.y - 18.0),
                Vec2::new(18.0, 18.0),
            );
            let popup_id = Id::new(("insert_overflow", info.id));
            let response = ui.put(button_rect, egui::Button::new("⋯"));
            if response.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            egui::popup::popup_below_widget(ui, popup_id, &response, |ui| {
                ui.set_min_width(180.0);
                ui.heading("Inserts");
                ui.separator();
                for slot in 0..total {
                    let label = self.labels.get(slot).map(|s| s.as_str()).unwrap_or("");
                    let bypassed = api.insert_is_bypassed(strip_index, slot);
                    let row = draw_insert_row(ui, slot, label, bypassed, self.theme, false, false);
                    if row.clicked() {
                        api.insert_toggle_bypass(strip_index, slot);
                    }
                }
            });
        }

        if let Some(from) = dragging {
            if !ui.ctx().input(|i| i.pointer.primary_down()) {
                if let Some(to) = drop_target {
                    if from != to {
                        api.insert_move(strip_index, from, to);
                    }
                }
                dragging = None;
            } else {
                ui.ctx().request_repaint();
            }
        }

        ui.ctx().data_mut(|d| d.insert_temp(drag_id, dragging));
    }
}

fn draw_insert_row(
    ui: &mut Ui,
    slot: usize,
    label: &str,
    bypassed: bool,
    theme: &MixerTheme,
    dragging: bool,
    is_target: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 20.0),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let mut fill = theme.inactive_slot;
    if is_target {
        fill = theme.active_slot.linear_multiply(1.2);
    }
    if dragging {
        fill = theme.active_slot;
    }
    painter.rect(
        rect,
        theme.rounding_small,
        fill,
        Stroke::new(1.0, theme.slot_border.color),
    );

    let bypass_color = if bypassed { theme.muted } else { theme.accent };
    let dot_center = pos2(rect.min.x + 10.0, rect.center().y);
    painter.circle(dot_center, 4.0, bypass_color, Stroke::NONE);

    painter.text(
        pos2(rect.min.x + 22.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{:02}. {}", slot + 1, label),
        egui::FontId::proportional(11.0),
        theme.header_text,
    );

    response
}
