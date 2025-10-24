use eframe::egui::{self, Align, Response, RichText, Ui};

use crate::ui::mixer::theme::MixerTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Insert,
    Send,
}

pub struct SlotView<'a> {
    pub index: usize,
    pub label: &'a str,
    pub active: bool,
    pub pre_fader: bool,
    pub kind: SlotKind,
}

impl<'a> SlotView<'a> {
    pub fn show(self, ui: &mut Ui, theme: &MixerTheme) -> Response {
        let label = if self.label.is_empty() {
            match self.kind {
                SlotKind::Insert => "(empty)",
                SlotKind::Send => "(no send)",
            }
        } else {
            self.label
        };

        let frame = egui::Frame::none()
            .fill(if self.active {
                theme.active_slot
            } else {
                theme.inactive_slot
            })
            .stroke(theme.slot_border)
            .rounding(theme.rounding_small);

        let egui::InnerResponse { response, .. } = frame.show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(2.0);
                ui.label(RichText::new(label).size(11.0).color(theme.header_text));
                ui.add_space(2.0);
                let status = if self.pre_fader { "PRE" } else { "POST" };
                let status_color = if self.pre_fader {
                    theme.accent
                } else {
                    theme.header_text
                };
                ui.with_layout(egui::Layout::top_down(Align::Center), |ui| {
                    ui.label(RichText::new(status).color(status_color).size(10.0));
                });
                ui.add_space(1.0);
            });
        });

        let mut response = response;

        if ui.ui_contains_pointer() && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        response = response.on_hover_text(format!(
            "{} {}\nDouble-click: rename/replace\nRight-click: more options",
            match self.kind {
                SlotKind::Insert => "Insert",
                SlotKind::Send => "Send",
            },
            self.index + 1
        ));

        response
            .context_menu(|ui| {
                ui.label(RichText::new(label).strong());
                ui.separator();
                if ui.button("Bypass").clicked() {
                    ui.close_menu();
                }
                if ui.button("Toggle Pre/Post").clicked() {
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Replace...").clicked() {
                    ui.close_menu();
                }
                if ui.button("Remove").clicked() {
                    ui.close_menu();
                }
            })
            .response
    }
}

pub fn overflow_button(ui: &mut Ui, count: usize, hidden: usize, theme: &MixerTheme) -> Response {
    let text = format!("+{}", hidden);
    ui.add(
        egui::Button::new(RichText::new(text).color(theme.header_text))
            .fill(theme.strip_bg)
            .rounding(theme.rounding_small)
            .stroke(theme.strip_border),
    )
    .on_hover_text(format!("{count} total slots"))
}
