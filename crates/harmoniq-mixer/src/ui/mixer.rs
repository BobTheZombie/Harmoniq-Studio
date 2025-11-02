use crate::state::*;
use harmoniq_ui::{Fader, HarmoniqPalette, Knob, LevelMeter, StateToggleButton};
use std::collections::BTreeSet;

use egui::{self, Align, Color32, Layout, Margin, RichText, Rounding, Stroke};

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
    pub pan_knob_diameter: f32,
    pub send_knob_diameter: f32,
}

impl Default for StripMetrics {
    fn default() -> Self {
        Self {
            fader_h: 216.0,
            meter_w: 26.0,
            strip_w: 160.0,
            section_spacing: 10.0,
            pan_knob_diameter: 54.0,
            send_knob_diameter: 44.0,
        }
    }
}

pub fn render(ui: &mut egui::Ui, props: crate::MixerProps) {
    let crate::MixerProps {
        state,
        callbacks,
        palette,
    } = props;

    ui.vertical(|ui| {
        header(ui, state, palette);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Routing Matrix").clicked() {
                state.routing_visible = !state.routing_visible;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new("Shift+double-click any meter to clear peaks")
                        .small()
                        .color(palette.text_muted),
                );
            });
        });
        ui.add_space(6.0);

        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 0.0);
                let mut reset_requests: Vec<ResetRequest> = Vec::new();
                let selected = state.selected;
                ui.horizontal_top(|ui| {
                    for channel in &mut state.channels {
                        let request = channel_strip(
                            ui,
                            channel,
                            callbacks,
                            palette,
                            selected == Some(channel.id),
                        );
                        if let Some(request) = request {
                            reset_requests.push(request);
                        }
                    }
                });

                let reset_all = reset_requests
                    .iter()
                    .any(|request| matches!(request, ResetRequest::All));
                if reset_all {
                    state.reset_peaks_all();
                } else {
                    for request in reset_requests {
                        if let ResetRequest::Channel(id) = request {
                            state.reset_peaks_for(id);
                        }
                    }
                }
            });

        if state.routing_visible {
            routing_matrix_window(ui, callbacks, state, palette);
        }
    });
}

fn header(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mixer").heading().color(palette.text_primary));
        if let Some(selected_id) = state.selected {
            if let Some(channel) = state.channels.iter().find(|ch| ch.id == selected_id) {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(format!("Selected: {}", channel.name))
                        .strong()
                        .color(palette.text_primary),
                );
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("Tip: Drag faders to adjust; double-click to reset")
                    .small()
                    .color(palette.text_muted),
            );
        });
    });
}

fn strip_fill(palette: &HarmoniqPalette, is_selected: bool, channel: &Channel) -> Color32 {
    if channel.solo {
        palette.mixer_strip_solo
    } else if channel.mute {
        palette.mixer_strip_muted
    } else if is_selected {
        palette.mixer_strip_selected
    } else {
        palette.mixer_strip_bg
    }
}

fn channel_strip(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    is_selected: bool,
) -> Option<ResetRequest> {
    let metrics = StripMetrics::default();
    let mut reset_request = None;

    let strip_response = egui::Frame::none()
        .fill(strip_fill(palette, is_selected, channel))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(metrics.strip_w);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, metrics.section_spacing);

            let mut name = channel.name.clone();
            let name_response = egui::TextEdit::singleline(&mut name)
                .desired_width(ui.available_width())
                .font(egui::TextStyle::Monospace);
            if ui.add(name_response).lost_focus() {
                channel.name = name;
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
                let meter = &channel.meter;
                let rms = 0.5 * (meter.rms_l + meter.rms_r);
                let meter_response = ui.add(
                    LevelMeter::new(palette)
                        .with_levels(meter.peak_l, meter.peak_r, rms)
                        .with_size(egui::vec2(metrics.meter_w, metrics.fader_h))
                        .with_clip(meter.clip_l, meter.clip_r)
                        .interactive(true),
                );
                if meter_response.double_clicked() {
                    let all = ui.input(|input| input.modifiers.shift);
                    reset_request = Some(if all {
                        ResetRequest::All
                    } else {
                        ResetRequest::Channel(channel.id)
                    });
                }

                ui.vertical(|ui| {
                    let fader_response = ui.add(
                        Fader::new(&mut channel.gain_db, -60.0, 12.0, 0.0, palette)
                            .with_height(metrics.fader_h),
                    );
                    if fader_response.changed() {
                        (callbacks.set_gain_pan)(channel.id, channel.gain_db, channel.pan);
                    }
                    ui.add_space(4.0);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{:.1} dB", channel.gain_db))
                                .small()
                                .color(palette.text_muted),
                        );
                    });
                });
            });

            ui.add_space(4.0);
            inserts_panel(ui, channel, callbacks, palette, &metrics);

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    let pan_response = ui.add(
                        Knob::new(&mut channel.pan, -1.0, 1.0, 0.0, "Pan", palette)
                            .with_diameter(metrics.pan_knob_diameter),
                    );
                    if pan_response.changed() {
                        (callbacks.set_gain_pan)(channel.id, channel.gain_db, channel.pan);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    let mute_response = ui
                        .add(
                            StateToggleButton::new(&mut channel.mute, "M", palette)
                                .with_width(36.0),
                        )
                        .on_hover_text("Mute");
                    if mute_response.changed() {
                        (callbacks.set_mute)(channel.id, channel.mute);
                    }
                    let solo_response = ui
                        .add(
                            StateToggleButton::new(&mut channel.solo, "S", palette)
                                .with_width(36.0),
                        )
                        .on_hover_text("Solo");
                    if solo_response.changed() {
                        (callbacks.set_solo)(channel.id, channel.solo);
                    }
                });
            });

            if !channel.is_master {
                ui.separator();
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    ui.label(RichText::new("Sends").small().color(palette.text_muted));
                    for send in &mut channel.sends {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                            ui.label(
                                RichText::new(format!("{}", (b'A' + send.id) as char))
                                    .small()
                                    .color(palette.text_primary),
                            );
                            let send_response = ui.add(
                                Knob::new(&mut send.level, 0.0, 1.0, 0.0, "", palette)
                                    .with_diameter(metrics.send_knob_diameter),
                            );
                            if send_response.changed() {
                                (callbacks.configure_send)(channel.id, send.id, send.level);
                            }
                        });
                    }
                });
            }
        })
        .response;

    strip_response.context_menu(|ui| {
        if ui.button("Add Insert…").clicked() {
            (callbacks.open_insert_browser)(channel.id, None);
            ui.close_menu();
        }
    });

    reset_request
}

fn inserts_panel(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    metrics: &StripMetrics,
) {
    let drag_id = egui::Id::new(("mixer_insert_drag", channel.id));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        ui.label(RichText::new("Inserts").small().color(palette.text_muted));

        let pointer_pos = ui.ctx().pointer_interact_pos();
        let mut drop_target: Option<usize> = None;
        let mut pending_move: Option<(usize, usize)> = None;

        for (index, slot) in channel.inserts.iter_mut().enumerate() {
            let title = if slot.name.is_empty() {
                "Empty".to_string()
            } else {
                slot.name.clone()
            };

            let slot_fill = if slot.name.is_empty() {
                palette.mixer_slot_bg
            } else {
                palette.mixer_slot_active
            };

            let mut drop_request = None;
            let frame = egui::Frame::none()
                .fill(slot_fill)
                .stroke(Stroke::new(1.0, palette.mixer_slot_border))
                .rounding(Rounding::same(8.0))
                .inner_margin(Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                        let handle = ui
                            .add(egui::Label::new("≡").sense(egui::Sense::drag()))
                            .on_hover_text("Drag to reorder");
                        if handle.drag_started() {
                            ui.ctx().data_mut(|data| data.insert_temp(drag_id, index));
                        }
                        if handle.drag_stopped() {
                            if let Some(from) =
                                ui.ctx().data(|data| data.get_temp::<usize>(drag_id))
                            {
                                drop_request = Some((from, drop_target.unwrap_or(index)));
                            }
                            ui.ctx().data_mut(|data| data.remove::<usize>(drag_id));
                        }

                        let bypass_response = ui
                            .add(
                                StateToggleButton::new(&mut slot.bypass, "Byp", palette)
                                    .with_width(52.0),
                            )
                            .on_hover_text("Bypass");
                        if bypass_response.changed() {
                            (callbacks.set_insert_bypass)(channel.id, index, slot.bypass);
                        }

                        if ui
                            .add_sized(
                                [ui.available_width() - 64.0, 28.0],
                                egui::Button::new(
                                    RichText::new(title.clone())
                                        .small()
                                        .color(palette.text_primary),
                                ),
                            )
                            .clicked()
                        {
                            (callbacks.open_insert_ui)(channel.id, index);
                        }

                        if ui
                            .add(egui::Button::new("✕").fill(palette.toolbar_highlight))
                            .on_hover_text("Remove")
                            .clicked()
                        {
                            (callbacks.remove_insert)(channel.id, index);
                        }
                    });
                });

            let response = frame.response;
            if let Some((from, to)) = drop_request {
                pending_move = Some((from, to));
            }

            if let Some(pos) = pointer_pos {
                if response.rect.contains(pos) {
                    drop_target = Some(index);
                    let stroke = Stroke::new(1.5, palette.accent_alt);
                    ui.painter()
                        .rect_stroke(response.rect.shrink(2.0), 6.0, stroke);
                }
            }
        }

        if ui
            .ctx()
            .data(|data| data.get_temp::<usize>(drag_id))
            .is_some()
            && drop_target.is_none()
        {
            drop_target = Some(channel.inserts.len());
        }

        if let Some((from, to)) = pending_move {
            if from != to && from < channel.inserts.len() {
                let mut destination = to.min(channel.inserts.len());
                let slot = channel.inserts.remove(from);
                if destination > from {
                    destination = destination.saturating_sub(1);
                }
                destination = destination.min(channel.inserts.len());
                channel.inserts.insert(destination, slot);
                (callbacks.reorder_insert)(channel.id, from, destination);
            }
        }

        if ui
            .add_sized(
                [metrics.strip_w - 12.0, 32.0],
                egui::Button::new(
                    RichText::new("+ Add Insert")
                        .small()
                        .color(palette.text_primary),
                )
                .fill(palette.toolbar_highlight)
                .stroke(Stroke::new(1.0, palette.toolbar_outline)),
            )
            .clicked()
        {
            (callbacks.open_insert_browser)(channel.id, None);
        }
    });
}

fn routing_matrix_window(
    ui: &mut egui::Ui,
    callbacks: &mut crate::MixerCallbacks,
    state: &mut MixerState,
    palette: &HarmoniqPalette,
) {
    let mut open = state.routing_visible;
    egui::Window::new("Routing Matrix")
        .open(&mut open)
        .collapsible(false)
        .default_size(egui::vec2(720.0, 420.0))
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Level 0..1. Click a cell to toggle; drag to adjust.")
                        .small()
                        .color(palette.text_muted),
                );
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
                .spacing(egui::vec2(12.0, 6.0))
                .show(ui, |grid_ui| {
                    grid_ui.label(RichText::new("Source").strong().color(palette.text_primary));
                    for bus in &buses {
                        grid_ui.label(RichText::new(bus).strong().color(palette.text_primary));
                    }
                    grid_ui.end_row();

                    let mut delta = RoutingDelta::default();
                    for channel in state.channels.iter().filter(|channel| !channel.is_master) {
                        grid_ui
                            .label(RichText::new(channel.name.clone()).color(palette.text_primary));
                        for bus in &buses {
                            let current = state.routing.level(channel.id, bus).unwrap_or(0.0);
                            let cell_id = grid_ui.make_persistent_id(("route", channel.id, bus));
                            let (rect, response) = grid_ui.allocate_exact_size(
                                egui::vec2(92.0, 28.0),
                                egui::Sense::click_and_drag(),
                            );
                            let painter = grid_ui.painter_at(rect);
                            let fill = if current > 0.0 {
                                palette.accent_soft
                            } else {
                                palette.mixer_slot_bg
                            };
                            painter.rect_filled(rect, 6.0, fill);
                            painter.rect_stroke(
                                rect,
                                6.0,
                                Stroke::new(1.0, palette.mixer_strip_border),
                            );
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("{current:.2}"),
                                egui::TextStyle::Small.resolve(grid_ui.style()),
                                palette.text_primary,
                            );

                            let mut level = current;
                            if response.clicked() {
                                if level == 0.0 {
                                    level = 1.0;
                                    delta.set.push((channel.id, bus.clone(), level));
                                } else {
                                    delta.remove.push((channel.id, bus.clone()));
                                    level = 0.0;
                                }
                            }
                            if response.dragged() {
                                let dy = response.drag_delta().y;
                                if dy.abs() > f32::EPSILON {
                                    level = (level - dy * 0.01).clamp(0.0, 1.0);
                                    delta.set.push((channel.id, bus.clone(), level));
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
