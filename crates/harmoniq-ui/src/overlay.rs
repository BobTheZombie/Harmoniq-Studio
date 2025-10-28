use egui::{self, Align2};

pub fn startup_banner(ctx: &egui::Context, visible: &mut bool, progress: Option<f32>, text: &str) {
    if !*visible {
        return;
    }

    if let Some(value) = progress {
        if value >= 1.0 {
            *visible = false;
            return;
        }
    }

    egui::Area::new("startup_banner")
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 32.0))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            let visuals = ui.visuals().clone();
            let rounding = visuals.window_rounding;
            let stroke = visuals.widgets.noninteractive.bg_stroke;
            let fill = visuals.popup_fill;

            egui::Frame::none()
                .fill(fill)
                .stroke(stroke)
                .rounding(rounding)
                .shadow(visuals.popup_shadow)
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .show(ui, |ui| {
                    ui.set_width(320.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(egui::RichText::new(text).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let close = ui.add(
                                            egui::Button::new(egui::RichText::new("✕").strong())
                                                .frame(false),
                                        );
                                        if close.clicked() {
                                            *visible = false;
                                        }
                                    },
                                );
                            },
                        );

                        if let Some(value) = progress {
                            ui.add(egui::ProgressBar::new(value.clamp(0.0, 1.0)).show_percentage());
                        }
                    });
                });
        });
}
