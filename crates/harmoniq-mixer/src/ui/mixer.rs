use super::widgets::*;
use crate::state::*;
use std::collections::BTreeSet;

use egui::{self, RichText};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResetRequest {
    Channel(ChannelId),
    All,
}

pub struct StripMetrics {
    pub fader_h: f32,
    pub meter_w: f32,
    pub strip_w: f32,
    pub section_spacing: f32,
}

impl Default for StripMetrics {
    fn default() -> Self {
        Self {
            fader_h: 180.0,
            meter_w: 6.0,
            strip_w: 96.0,
            section_spacing: 4.0,
        }
    }
}

pub fn render(ui: &mut egui::Ui, props: crate::MixerProps) {
    let crate::MixerProps { state, callbacks } = props;
    ui.vertical(|ui| {
        header(ui, &mut *state);
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Routing Matrix").clicked() {
                state.routing_visible = !state.routing_visible;
            }
        });
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let mut reset_requests: Vec<ResetRequest> = Vec::new();
                    for idx in 0..state.channels.len() {
                        let request = {
                            let ch = &mut state.channels[idx];
                            channel_strip(ui, ch, &mut *callbacks)
                        };
                        if let Some(req) = request {
                            reset_requests.push(req);
                        }
                    }
                    let mut reset_all = false;
                    for req in &reset_requests {
                        if matches!(req, ResetRequest::All) {
                            reset_all = true;
                            break;
                        }
                    }
                    if reset_all {
                        state.reset_peaks_all();
                    } else {
                        for req in reset_requests {
                            if let ResetRequest::Channel(id) = req {
                                state.reset_peaks_for(id);
                            }
                        }
                    }
                });
            });
        if state.routing_visible {
            routing_matrix_window(ui, callbacks, &mut *state);
        }
    });
}

fn header(ui: &mut egui::Ui, state: &mut MixerState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mixer").heading());
        ui.add_space(8.0);
        if let Some(sel) = state.selected {
            if let Some(ch) = state.channels.iter_mut().find(|c| c.id == sel) {
                ui.label(format!("Selected: {}", ch.name));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small("Tip: Shift+Drag fader = fine; Double-click to reset");
        });
    });
}

fn channel_strip(
    ui: &mut egui::Ui,
    ch: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
) -> Option<ResetRequest> {
    let metrics = StripMetrics::default();
    let mut reset = None;
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(6.0))
        .rounding(egui::Rounding::same(4.0))
        .show(ui, |ui| {
            ui.set_width(metrics.strip_w);
            ui.spacing_mut().item_spacing = egui::vec2(4.0, metrics.section_spacing);
            // NAME
            {
                let mut name = ch.name.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut name)
                            .desired_width(metrics.strip_w - 12.0)
                            .font(egui::TextStyle::Monospace),
                    )
                    .lost_focus()
                {
                    ch.name = name;
                }
            }
            // METERS + FADER row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                ui.vertical(|ui| {
                    let m = &ch.meter;
                    let resp_l = meter_vertical(
                        ui,
                        m.peak_l,
                        m.rms_l,
                        m.peak_hold_l,
                        m.clip_l,
                        egui::vec2(metrics.meter_w, metrics.fader_h),
                    );
                    let resp_r = meter_vertical(
                        ui,
                        m.peak_r,
                        m.rms_r,
                        m.peak_hold_r,
                        m.clip_r,
                        egui::vec2(metrics.meter_w, metrics.fader_h),
                    );
                    if resp_l.double_clicked() || resp_r.double_clicked() {
                        let all = ui.input(|i| i.modifiers.shift);
                        reset = Some(if all {
                            ResetRequest::All
                        } else {
                            ResetRequest::Channel(ch.id)
                        });
                    }
                });
                // fader
                let mut db = ch.gain_db;
                if fader_db(ui, &mut db, -60.0..=12.0, metrics.fader_h) {
                    ch.gain_db = db;
                    (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                }
            });
            // PAN + M/S
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("PAN").small());
                let mut pan = ch.pan;
                if small_knob(ui, &mut pan, -1.0..=1.0, "") {
                    ch.pan = pan;
                    (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                }
                ui.separator();
                if ui
                    .add_sized([24.0, 18.0], egui::SelectableLabel::new(ch.mute, "M"))
                    .on_hover_text("Mute")
                    .clicked()
                {
                    ch.mute = !ch.mute;
                    (callbacks.set_mute)(ch.id, ch.mute);
                }
                if ui
                    .add_sized([24.0, 18.0], egui::SelectableLabel::new(ch.solo, "S"))
                    .on_hover_text("Solo")
                    .clicked()
                {
                    ch.solo = !ch.solo;
                    (callbacks.set_solo)(ch.id, ch.solo);
                }
            });
            ui.separator();
            // INSERTS
            inserts_panel(ui, ch, callbacks, &metrics);
            if !ch.is_master {
                ui.separator();
                // SENDS
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    ui.label(RichText::new("SENDS").small());
                    for s in &mut ch.sends {
                        ui.horizontal(|ui| {
                            let label = format!("{}", ((b'A' + s.id) as char));
                            ui.label(RichText::new(label).small());
                            let mut lvl = s.level;
                            if small_knob(ui, &mut lvl, 0.0..=1.0, "") {
                                s.level = lvl;
                                (callbacks.configure_send)(ch.id, s.id, s.level);
                            }
                        });
                    }
                });
            }
            ui.add_space(6.0);
            if ui.button("Select").clicked() {
                // selection is stored on MixerState outside; handled by caller if needed
            }
        })
        .response
        .context_menu(|ui| {
            if ui.button("Add Insert…").clicked() {
                (callbacks.open_insert_browser)(ch.id, None);
                ui.close_menu();
            }
        });
    reset
}

fn inserts_panel(
    ui: &mut egui::Ui,
    ch: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    metrics: &StripMetrics,
) {
    let drag_id = egui::Id::new(("mixer_insert_drag", ch.id));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        ui.label(RichText::new("INSERTS").small());

        let pointer_pos = ui.ctx().pointer_interact_pos();
        let mut drop_target: Option<usize> = None;
        let mut pending_move: Option<(usize, usize)> = None;

        for idx in 0..ch.inserts.len() {
            let insert_label = if ch.inserts[idx].name.is_empty() {
                "Empty".to_string()
            } else {
                ch.inserts[idx].name.clone()
            };
            let mut row_rect = egui::Rect::NOTHING;
            let mut drop_request = None;
            let response = ui
                .horizontal(|ui| {
                    let handle = ui
                        .add_sized([18.0, 18.0], egui::Label::new("≡").sense(egui::Sense::drag()))
                        .on_hover_text("Drag to reorder");
                    if handle.drag_started() {
                        ui.ctx().data_mut(|data| data.insert_temp(drag_id, idx));
                    }
                    if handle.drag_released() {
                        if let Some(from) = ui.ctx().data(|data| data.get_temp::<usize>(drag_id)) {
                            drop_request = Some((from, drop_target.unwrap_or(idx)));
                        }
                        ui.ctx().data_mut(|data| data.remove::<usize>(drag_id));
                    }

                    let mut bypass = ch.inserts[idx].bypass;
                    if ui
                        .add_sized([18.0, 18.0], egui::SelectableLabel::new(bypass, "⏸"))
                        .on_hover_text("Bypass")
                        .clicked()
                    {
                        bypass = !bypass;
                        ch.inserts[idx].bypass = bypass;
                        (callbacks.set_insert_bypass)(ch.id, idx, bypass);
                    }
                    if ui
                        .add_sized(
                            [metrics.strip_w - 64.0, 18.0],
                            egui::Button::new(RichText::new(insert_label).small()),
                        )
                        .clicked()
                    {
                        (callbacks.open_insert_ui)(ch.id, idx);
                    }
                    if ui
                        .add_sized([18.0, 18.0], egui::Button::new("✕"))
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        (callbacks.remove_insert)(ch.id, idx);
                    }
                })
                .response;
            row_rect = response.rect;
            if let Some((from, target)) = drop_request {
                pending_move = Some((from, target));
            }

            if let Some(pos) = pointer_pos {
                if row_rect.contains(pos) {
                    drop_target = Some(idx);
                    let stroke = egui::Stroke::new(1.0, ui.visuals().selection.stroke.color);
                    ui.painter().rect_stroke(row_rect.shrink(1.0), 2.0, stroke);
                }
            }
        }

        if ui.ctx().data(|data| data.get_temp::<usize>(drag_id)).is_some()
            && drop_target.is_none()
        {
            drop_target = Some(ch.inserts.len());
        }

        if let Some((from, to)) = pending_move {
            if from != to && from < ch.inserts.len() {
                let mut destination = to.min(ch.inserts.len());
                let slot = ch.inserts.remove(from);
                if destination > from {
                    destination = destination.saturating_sub(1);
                }
                destination = destination.min(ch.inserts.len());
                ch.inserts.insert(destination, slot);
                (callbacks.reorder_insert)(ch.id, from, destination);
            }
        }

        if ui
            .add_sized(
                [metrics.strip_w - 12.0, 18.0],
                egui::Button::new(RichText::new("+ Add Insert").small()),
            )
            .clicked()
        {
            (callbacks.open_insert_browser)(ch.id, None);
        }
    });
}

fn routing_matrix_window(
    ui: &mut egui::Ui,
    callbacks: &mut crate::MixerCallbacks,
    state: &mut MixerState,
) {
    let mut open = state.routing_visible;
    egui::Window::new("Routing Matrix")
        .open(&mut open)
        .collapsible(false)
        .default_size(egui::vec2(720.0, 420.0))
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Level 0..1. Click cell to toggle; drag to adjust.");
                if ui.button("Close").clicked() {
                    state.routing_visible = false;
                }
            });
            ui.separator();

            let mut buses: BTreeSet<String> = BTreeSet::new();
            buses.insert("MASTER".to_string());
            for map in state.routing.routes.values() {
                for bus in map.keys() {
                    buses.insert(bus.clone());
                }
            }

            egui::Grid::new("routing_matrix_grid")
                .striped(true)
                .show(ui, |grid_ui| {
                    grid_ui.label(RichText::new("Source").strong());
                    for bus in &buses {
                        grid_ui.label(RichText::new(bus).strong());
                    }
                    grid_ui.end_row();

                    let mut delta = RoutingDelta::default();
                    for ch in state.channels.iter().filter(|c| !c.is_master) {
                        grid_ui.label(ch.name.clone());
                        for bus in &buses {
                            let current = state.routing.level(ch.id, bus).unwrap_or(0.0);
                            let cell_id = grid_ui.make_persistent_id(("route", ch.id, bus));
                            let (rect, _) =
                                grid_ui.allocate_exact_size(egui::vec2(80.0, 22.0), egui::Sense::click_and_drag());
                            let painter = grid_ui.painter_at(rect);
                            let bg = if current > 0.0 {
                                grid_ui.visuals().selection.bg_fill
                            } else {
                                grid_ui.visuals().faint_bg_color
                            };
                            painter.rect_filled(rect, 3.0, bg);
                            painter.rect_stroke(
                                rect,
                                3.0,
                                egui::Stroke::new(
                                    1.0,
                                    grid_ui
                                        .visuals()
                                        .widgets
                                        .noninteractive
                                        .fg_stroke
                                        .color,
                                ),
                            );
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("{current:.2}"),
                                egui::TextStyle::Small.resolve(grid_ui.style()),
                                grid_ui.visuals().text_color(),
                            );

                            let response =
                                grid_ui.interact(rect, cell_id, egui::Sense::click_and_drag());
                            let mut level = current;
                            if response.clicked() {
                                if level == 0.0 {
                                    level = 1.0;
                                    delta.set.push((ch.id, bus.clone(), level));
                                } else {
                                    delta.remove.push((ch.id, bus.clone()));
                                    level = 0.0;
                                }
                            }
                            if response.dragged() {
                                let dy = response.drag_delta().y;
                                if dy.abs() > f32::EPSILON {
                                    level = (level - dy * 0.01).clamp(0.0, 1.0);
                                    delta.set.push((ch.id, bus.clone(), level));
                                }
                            }
                        }
                        grid_ui.end_row();
                    }

                    if !delta.set.is_empty() || !delta.remove.is_empty() {
                        state.routing.apply_delta(&delta);
                        (callbacks.apply_routing)(delta);
                    }
                });
        });
    state.routing_visible = open;
}
