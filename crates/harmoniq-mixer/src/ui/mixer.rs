use crate::state::{Channel, ChannelId, MixerState, RoutingDelta, MAX_INSERT_SLOTS};
use egui::{
    self, Align, Color32, ComboBox, Frame, Id, Layout, Margin, Pos2, RichText, Rounding, Sense,
    Stroke, TextStyle, Vec2,
};
use harmoniq_ui::{Fader, HarmoniqPalette, Knob, LevelMeter, StateToggleButton};

pub fn render(ui: &mut egui::Ui, props: crate::MixerProps) {
    let crate::MixerProps {
        state,
        callbacks,
        palette,
    } = props;

    let strip_width = state.layout.strip_width.clamp(140.0, 260.0);
    let metrics = StripMetrics::scaled(strip_width);

    mixer_toolbar(ui, state, palette);

    ui.add_space(6.0);

    egui::ScrollArea::horizontal()
        .id_source("mixer_strip_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);

                let mut master: Option<usize> = None;
                for (idx, ch) in state.channels.iter().enumerate() {
                    if ch.is_master {
                        master = Some(idx);
                        continue;
                    }
                    strip_ui(ui, ch, state, callbacks, palette, &metrics, false);
                }

                if let Some(idx) = master {
                    strip_ui(
                        ui,
                        &state.channels[idx],
                        state,
                        callbacks,
                        palette,
                        &metrics,
                        true,
                    );
                }
            });
        });
}

#[derive(Clone, Copy, Debug)]
struct StripMetrics {
    strip_width: f32,
    meter_width: f32,
    fader_height: f32,
    knob_size: f32,
    send_knob: f32,
}

impl StripMetrics {
    fn scaled(strip_width: f32) -> Self {
        let scale = (strip_width / 188.0).clamp(0.6, 1.6);
        Self {
            strip_width,
            meter_width: 26.0 * scale,
            fader_height: 220.0 * scale,
            knob_size: 52.0 * scale,
            send_knob: 42.0 * scale,
        }
    }
}

fn mixer_toolbar(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 6.0);

        ui.label(
            RichText::new("Mixer")
                .heading()
                .color(palette.accent)
                .strong(),
        );

        let mut width = state.layout.strip_width;
        let slider = egui::Slider::new(&mut width, 140.0..=260.0)
            .text("Zoom")
            .step_by(2.0)
            .custom_formatter(|v, _| format!("{}%", ((v - 140.0) / 1.2).round()));
        if ui
            .add(slider)
            .on_hover_text("Resize mixer strips")
            .changed()
        {
            state.layout.strip_width = width;
        }

        ui.separator();

        ui.label(RichText::new("Group select (shift-click)").color(palette.text_muted));

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut routing = state.routing_visible;
            if ui
                .toggle_value(&mut routing, "Routing")
                .on_hover_text("Show routing controls")
                .clicked()
            {
                state.routing_visible = routing;
            }
        });
    });
}

fn strip_ui(
    ui: &mut egui::Ui,
    channel: &Channel,
    state: &mut MixerState,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    metrics: &StripMetrics,
    is_master: bool,
) {
    let mut channel = channel.clone();
    let hover_accent = Color32::from_rgb(70, 90, 120);
    let bg = if is_master {
        Color32::from_rgb(30, 32, 38)
    } else {
        palette.mixer_strip_bg
    };

    let frame = Frame::none()
        .fill(bg)
        .rounding(Rounding::same(10.0))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .inner_margin(Margin::symmetric(10.0, 8.0));

    frame.show(ui, |ui| {
        ui.set_width(metrics.strip_width);
        ui.set_min_height(metrics.fader_height + 260.0);

        let response = ui.vertical_centered(|ui| {
            header_ui(ui, &mut channel, state, palette, is_master);
            ui.add_space(6.0);
            meter_and_fader(ui, &mut channel, callbacks, palette, metrics);
            ui.add_space(6.0);
            transport_row(ui, &mut channel, callbacks, palette);
            ui.add_space(4.0);
            inserts_ui(ui, &mut channel, callbacks, palette);
            ui.add_space(4.0);
            sends_ui(ui, &mut channel, state, callbacks, palette, metrics);
            ui.add_space(6.0);
            routing_ui(ui, &mut channel, state, callbacks, palette);
        });

        if response.response.hovered() {
            ui.painter().rect(
                ui.min_rect(),
                12.0,
                Color32::from_rgba_premultiplied(
                    hover_accent.r(),
                    hover_accent.g(),
                    hover_accent.b(),
                    20,
                ),
                Stroke::new(1.0, hover_accent),
            );
        }

        state
            .channels
            .iter_mut()
            .find(|ch| ch.id == channel.id)
            .map(|ch| *ch = channel);
    });
}

fn header_ui(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    state: &mut MixerState,
    palette: &HarmoniqPalette,
    is_master: bool,
) {
    ui.horizontal(|ui| {
        let color = Color32::from_rgb(channel.color[0], channel.color[1], channel.color[2]);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::click());
        ui.painter()
            .rect_filled(rect.expand(1.0), 4.0, color.gamma_multiply(1.1));
        if resp.clicked() {
            channel.color = [color.r(), color.g(), color.b()];
        }
        resp.on_hover_text("Track color");

        let mut name = channel.name.clone();
        let text = egui::TextEdit::singleline(&mut name)
            .desired_width(ui.available_width())
            .hint_text("Track name")
            .font(TextStyle::Monospace);
        let response = ui.add(text).on_hover_text("Rename track");
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            channel.name = name;
        } else if response.changed() {
            channel.name = name;
        }

        let clicked = response.clicked();
        if clicked {
            if ui.input(|i| i.modifiers.shift) {
                if !state.grouped.remove(&channel.id) {
                    state.grouped.insert(channel.id);
                }
            } else {
                state.selected = Some(channel.id);
            }
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
        ui.label(
            RichText::new(channel.input_bus.clone())
                .small()
                .color(palette.text_primary),
        );
        ui.label(RichText::new("→").color(palette.text_muted));
        ui.label(
            RichText::new(channel.output_bus.clone())
                .small()
                .color(palette.text_primary),
        );
        if is_master {
            ui.label(RichText::new("MASTER").color(palette.accent));
        }
    });
}

fn meter_and_fader(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    metrics: &StripMetrics,
) {
    ui.horizontal(|ui| {
        let meter = LevelMeter::new(palette)
            .with_size(Vec2::new(metrics.meter_width, metrics.fader_height))
            .with_levels(
                channel.meter.peak_l,
                channel.meter.peak_r,
                channel.meter.rms_l,
            )
            .with_clip(channel.meter.clip_l, channel.meter.clip_r);
        ui.add(meter).on_hover_text("Level meter with peak hold");

        ui.vertical(|ui| {
            let mut gain = channel.gain_db;
            if ui
                .add(Fader::vertical(palette, &mut gain).with_height(metrics.fader_height))
                .on_hover_text("Volume fader (dB)")
                .changed()
            {
                channel.gain_db = gain;
                (callbacks.set_gain_pan)(channel.id, gain, channel.pan);
            }

            ui.add_space(6.0);
            let mut pan = channel.pan;
            if ui
                .add(
                    Knob::new(&mut pan)
                        .with_size(metrics.knob_size)
                        .with_palette(palette),
                )
                .on_hover_text("Pan")
                .changed()
            {
                channel.pan = pan;
                (callbacks.set_gain_pan)(channel.id, channel.gain_db, pan);
            }
        });
    });
}

fn transport_row(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
) {
    ui.horizontal(|ui| {
        let mute = ui
            .add(StateToggleButton::new(&mut channel.mute, "Mute", palette))
            .on_hover_text("Mute channel");
        if mute.changed() {
            (callbacks.set_mute)(channel.id, channel.mute);
        }

        let solo = ui
            .add(StateToggleButton::new(&mut channel.solo, "Solo", palette))
            .on_hover_text("Solo channel");
        if solo.changed() {
            (callbacks.set_solo)(channel.id, channel.solo);
        }

        ui.add(StateToggleButton::new(
            &mut channel.record_enable,
            "Arm",
            palette,
        ))
        .on_hover_text("Arm for recording");
    });
}

fn inserts_ui(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
) {
    ui.label(
        RichText::new("FX Inserts")
            .small()
            .color(palette.text_muted),
    );
    ui.add_space(2.0);

    let drag_id = Id::new(("insert_drag", channel.id));
    let mut drop_target = None;
    let mut pending_move: Option<(usize, usize)> = None;

    channel.ensure_insert_slot(MAX_INSERT_SLOTS - 1);

    for (slot_idx, slot) in channel
        .inserts
        .iter_mut()
        .enumerate()
        .take(MAX_INSERT_SLOTS)
    {
        let mut drop_here = false;
        let is_dragged = ui
            .ctx()
            .data(|d| d.get_temp::<usize>(drag_id) == Some(slot_idx));

        let slot_bg = if slot.plugin_uid.is_some() {
            palette.mixer_slot_active
        } else {
            palette.mixer_slot_bg
        };

        let resp = Frame::none()
            .fill(slot_bg)
            .stroke(Stroke::new(1.0, palette.mixer_slot_border))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let drag = ui
                        .label(RichText::new("≡").color(palette.text_muted))
                        .interact(Sense::drag())
                        .on_hover_text("Drag to reorder");
                    if drag.drag_started() {
                        ui.ctx().data_mut(|d| d.insert_temp(drag_id, slot_idx));
                    }
                    if drag.drag_stopped() {
                        if let Some(from) = ui.ctx().data(|d| d.get_temp::<usize>(drag_id)) {
                            let to = drop_target.unwrap_or(slot_idx);
                            pending_move = Some((from, to));
                        }
                        ui.ctx().data_mut(|d| d.remove::<usize>(drag_id));
                    }

                    let mut bypass = slot.bypass;
                    if ui
                        .checkbox(&mut bypass, "Byp")
                        .on_hover_text("Toggle bypass")
                        .clicked()
                    {
                        slot.bypass = bypass;
                        (callbacks.set_insert_bypass)(channel.id, slot_idx, bypass);
                    }

                    ui.label(RichText::new(slot.name.clone()).color(palette.text_primary));

                    if ui.small_button("✕").on_hover_text("Remove").clicked() {
                        (callbacks.remove_insert)(channel.id, slot_idx);
                        slot.plugin_uid = None;
                        slot.name = "Empty".into();
                    }
                });
            })
            .response;

        if resp.hovered() {
            drop_here = true;
        }

        if !is_dragged && resp.hovered() {
            resp.on_hover_text(format!("Insert slot {}", slot_idx + 1));
        }

        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            let y = pointer.y;
            if resp.rect.contains(Pos2::new(resp.rect.center().x, y)) {
                drop_target = Some(slot_idx);
            }
        }

        if drop_here {
            ui.painter()
                .rect_stroke(resp.rect.expand(2.0), 6.0, Stroke::new(1.0, palette.accent));
        }
    }

    if let Some((from, to)) = pending_move {
        (callbacks.reorder_insert)(channel.id, from, to);
    }
}

fn sends_ui(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    state: &MixerState,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    metrics: &StripMetrics,
) {
    ui.add_space(4.0);
    ui.label(RichText::new("Sends").small().color(palette.text_muted));
    ui.add_space(2.0);

    for send in channel.sends.iter_mut() {
        Frame::none()
            .fill(palette.mixer_slot_bg)
            .rounding(Rounding::same(6.0))
            .stroke(Stroke::new(1.0, palette.mixer_slot_border))
            .inner_margin(Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ComboBox::from_id_source(("send_target", channel.id, send.id))
                        .selected_text(send.target.as_deref().unwrap_or("Select destination"))
                        .show_ui(ui, |ui| {
                            for target in state.channels.iter().filter(|c| c.id != channel.id) {
                                ui.selectable_value(
                                    &mut send.target,
                                    Some(target.name.clone()),
                                    target.name.clone(),
                                );
                            }
                        })
                        .response
                        .on_hover_text("Send destination");

                    ui.add_space(4.0);
                    let mut level = send.level;
                    if ui
                        .add(
                            Knob::new(&mut level)
                                .with_size(metrics.send_knob)
                                .with_palette(palette),
                        )
                        .on_hover_text("Send level")
                        .changed()
                    {
                        send.level = level;
                        (callbacks.configure_send)(channel.id, send.id, level, send.pre_fader);
                    }
                });
            });
    }
}

fn routing_ui(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    state: &mut MixerState,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
) {
    if !state.routing_visible {
        return;
    }

    ui.add_space(4.0);
    ui.label(RichText::new("Routing").small().color(palette.text_muted));

    let mut delta = RoutingDelta::default();

    ui.vertical(|ui| {
        let mut output = channel.output_bus.clone();
        if ComboBox::from_id_source(("route_out", channel.id))
            .selected_text(output.clone())
            .show_ui(ui, |ui| {
                for bus in state.channels.iter().filter(|c| c.id != channel.id) {
                    ui.selectable_value(&mut output, bus.name.clone(), bus.name.clone());
                }
            })
            .response
            .on_hover_text("Route to track")
            .changed()
        {
            delta.set.push((channel.id, output.clone(), 1.0));
            channel.output_bus = output;
        }
    });

    if !delta.set.is_empty() || !delta.remove.is_empty() {
        (callbacks.apply_routing)(delta);
    }
}
