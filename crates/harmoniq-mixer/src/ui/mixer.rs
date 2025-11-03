use crate::state::{
    AutomationMode, Channel, ChannelEq, ChannelId, ChannelRackState, CueSend, EqBand, EqFilterKind,
    MixerLayout, MixerRackVisibility, MixerState, MixerViewTab, RoutingDelta,
};
use harmoniq_ui::{Fader, HarmoniqPalette, Knob, LevelMeter, StateToggleButton};
use std::collections::BTreeSet;

use egui::{
    self, Align, Align2, Color32, ComboBox, Frame, Layout, Margin, Pos2, Rect, RichText, Rounding,
    Sense, Shape, Stroke, TextStyle, Vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResetRequest {
    Channel(ChannelId),
    All,
}

#[derive(Clone, Copy, Debug)]
struct StripMetrics {
    fader_h: f32,
    meter_w: f32,
    strip_w: f32,
    section_spacing: f32,
    pan_knob_diameter: f32,
    send_knob_diameter: f32,
    cue_knob_diameter: f32,
    quick_control_width: f32,
}

impl Default for StripMetrics {
    fn default() -> Self {
        Self {
            fader_h: 216.0,
            meter_w: 28.0,
            strip_w: 188.0,
            section_spacing: 12.0,
            pan_knob_diameter: 52.0,
            send_knob_diameter: 44.0,
            cue_knob_diameter: 40.0,
            quick_control_width: 60.0,
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
        top_toolbar(ui, state, palette);
        ui.separator();

        match state.view_tab {
            MixerViewTab::MixConsole => mixconsole_view(ui, state, callbacks, palette),
            MixerViewTab::ChannelStrip => channel_strip_view(ui, state, palette),
            MixerViewTab::Meter => meter_view(ui, state, palette),
            MixerViewTab::ControlRoom => control_room_view(ui, state, palette),
        }
    });
}

fn top_toolbar(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
        for tab in [
            MixerViewTab::MixConsole,
            MixerViewTab::ChannelStrip,
            MixerViewTab::Meter,
            MixerViewTab::ControlRoom,
        ] {
            let label = match tab {
                MixerViewTab::MixConsole => "MixConsole",
                MixerViewTab::ChannelStrip => "Channel Strip",
                MixerViewTab::Meter => "Meter",
                MixerViewTab::ControlRoom => "Control Room",
            };
            let selected = state.view_tab == tab;
            let button = egui::SelectableLabel::new(selected, label);
            if ui.add(button).clicked() {
                state.view_tab = tab;
            }
        }

        ui.separator();
        ui.label(RichText::new("Search").small().color(palette.text_muted));
        ui.add(
            egui::TextEdit::singleline(&mut state.channel_filter)
                .desired_width(180.0)
                .hint_text("Channel, bus, color"),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            layout_toggle(
                ui,
                &mut state.layout,
                palette,
                |layout| &mut layout.show_right_zone,
                "Right Zone",
            );
            layout_toggle(
                ui,
                &mut state.layout,
                palette,
                |layout| &mut layout.show_left_zone,
                "Left Zone",
            );
            layout_toggle(
                ui,
                &mut state.layout,
                palette,
                |layout| &mut layout.show_meter_bridge,
                "Meter Bridge",
            );
            layout_toggle(
                ui,
                &mut state.layout,
                palette,
                |layout| &mut layout.show_channel_racks,
                "Channel Racks",
            );
        });
    });
}

fn layout_toggle(
    ui: &mut egui::Ui,
    layout: &mut MixerLayout,
    palette: &HarmoniqPalette,
    accessor: impl Fn(&mut MixerLayout) -> &mut bool,
    label: &str,
) {
    let value = accessor(layout);
    ui.add(StateToggleButton::new(value, label, palette).with_width(120.0));
}

fn mixconsole_view(
    ui: &mut egui::Ui,
    state: &mut MixerState,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
) {
    zone_toolbar(ui, state, palette);
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
        if state.layout.show_left_zone {
            Frame::none()
                .fill(palette.mixer_slot_bg)
                .inner_margin(Margin::symmetric(12.0, 16.0))
                .rounding(Rounding::same(12.0))
                .show(ui, |ui| {
                    ui.set_width(200.0);
                    render_left_zone(ui, state, palette);
                });
        }

        Frame::none().fill(Color32::TRANSPARENT).show(ui, |ui| {
            ui.vertical(|ui| {
                if state.layout.show_meter_bridge {
                    meter_bridge(ui, state, palette);
                    ui.add_space(10.0);
                }
                channel_area(ui, state, callbacks, palette);
            });
        });

        if state.layout.show_right_zone {
            Frame::none()
                .fill(palette.mixer_slot_bg)
                .inner_margin(Margin::symmetric(12.0, 16.0))
                .rounding(Rounding::same(12.0))
                .show(ui, |ui| {
                    ui.set_width(220.0);
                    render_right_zone(ui, state, palette);
                });
        }
    });
}

fn channel_strip_view(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    Frame::none()
        .fill(palette.mixer_strip_bg)
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::same(18.0))
        .show(ui, |ui| {
            if let Some(id) = state.selected {
                if let Some(channel) = state.channels.iter().find(|c| c.id == id) {
                    ui.heading(format!("Channel Strip: {}", channel.name));
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("The dedicated channel strip view is under construction.")
                            .color(palette.text_muted),
                    );
                }
            } else {
                ui.label(
                    RichText::new("Select a channel in the MixConsole to inspect its strip.")
                        .color(palette.text_muted),
                );
            }
        });
}

fn meter_view(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.heading(RichText::new("Meter Bridge").color(palette.text_primary));
        ui.add_space(12.0);
        meter_bridge(ui, state, palette);
    });
}

fn control_room_view(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    Frame::none()
        .fill(palette.mixer_strip_bg)
        .rounding(Rounding::same(16.0))
        .inner_margin(Margin::same(20.0))
        .show(ui, |ui| {
            ui.heading(RichText::new("Control Room").color(palette.text_primary));
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Configure monitor paths, cue mixes and talkback here. Routing hooks will become active once connected to the engine.",
                )
                .color(palette.text_muted),
            );
            ui.add_space(12.0);
            if state.channels.iter().any(|ch| ch.is_master) {
                ui.label(
                    RichText::new("Master bus is available — cue sends are fed from each channel's cue section.")
                        .small()
                        .color(palette.text_muted),
                );
            }
        });
}

fn zone_toolbar(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    Frame::none()
        .fill(palette.toolbar_highlight)
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .stroke(Stroke::new(1.0, palette.toolbar_outline))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Rack Visibility")
                        .strong()
                        .color(palette.text_primary),
                );
                rack_toggle(ui, palette, "Input", &mut state.rack_visibility.input);
                rack_toggle(ui, palette, "Pre", &mut state.rack_visibility.pre);
                rack_toggle(ui, palette, "Strip", &mut state.rack_visibility.strip);
                rack_toggle(ui, palette, "EQ", &mut state.rack_visibility.eq);
                rack_toggle(ui, palette, "Inserts", &mut state.rack_visibility.inserts);
                rack_toggle(ui, palette, "Sends", &mut state.rack_visibility.sends);
                rack_toggle(ui, palette, "Cue", &mut state.rack_visibility.cues);

                ui.separator();
                ui.label(
                    RichText::new("Utilities")
                        .strong()
                        .color(palette.text_primary),
                );
                if ui
                    .button(RichText::new("Reset Selected").color(palette.text_primary))
                    .clicked()
                {
                    if let Some(id) = state.selected {
                        state.reset_peaks_for(id);
                    }
                }
                if ui
                    .button(RichText::new("Reset All").color(palette.text_primary))
                    .clicked()
                {
                    state.reset_peaks_all();
                }
            });
        });
}

fn rack_toggle(ui: &mut egui::Ui, palette: &HarmoniqPalette, label: &str, value: &mut bool) {
    ui.add(StateToggleButton::new(value, label, palette).with_width(72.0));
}

fn render_left_zone(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.heading(RichText::new("Visibility").color(palette.text_primary));
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for channel in &mut state.channels {
                    let mut row = egui::CollapsingHeader::new(channel.name.clone())
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let color = Color32::from_rgb(
                                    channel.color[0],
                                    channel.color[1],
                                    channel.color[2],
                                );
                                let mut rect = ui.available_rect_before_wrap();
                                rect.set_height(6.0);
                                ui.painter().rect_filled(rect, 2.0, color);
                            });
                            ui.add_space(6.0);
                            ui.checkbox(&mut channel.visible, "Show in console");
                            ui.checkbox(&mut channel.record_enable, "Record Enable");
                            ui.checkbox(&mut channel.monitor_enable, "Monitor");
                            ui.checkbox(&mut channel.solo, "Solo");
                            ui.checkbox(&mut channel.mute, "Mute");
                        });
                    if row.header_response.clicked() {
                        state.selected = Some(channel.id);
                    }
                }
            });

        ui.add_space(12.0);
        ui.heading(RichText::new("Zones").color(palette.text_primary));
        ui.add_space(6.0);
        ui.checkbox(&mut state.layout.show_left_zone, "Left Zone");
        ui.checkbox(&mut state.layout.show_right_zone, "Right Zone");
        ui.checkbox(&mut state.layout.show_meter_bridge, "Meter Bridge");
        ui.checkbox(&mut state.layout.show_control_room, "Control Room");
    });
}

fn render_right_zone(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.heading(RichText::new("History").color(palette.text_primary));
        ui.add_space(6.0);
        ui.label(
            RichText::new("Automation and mix snapshots will appear here.")
                .small()
                .color(palette.text_muted),
        );
        ui.add_space(12.0);
        ui.heading(RichText::new("Snapshots").color(palette.text_primary));
        ui.add_space(6.0);
        ui.label(
            RichText::new("Create snapshots to store mix states.")
                .small()
                .color(palette.text_muted),
        );
        if ui.button("Capture Snapshot").clicked() {
            // hook for future automation
        }

        ui.add_space(16.0);
        ui.heading(RichText::new("Control Room").color(palette.text_primary));
        ui.add_space(6.0);
        if state.layout.show_control_room {
            ui.label(
                RichText::new("Control room monitoring active.")
                    .small()
                    .color(palette.text_muted),
            );
        } else {
            ui.label(
                RichText::new("Enable the Control Room in layout toggles.")
                    .small()
                    .color(palette.text_muted),
            );
        }
    });
}

fn meter_bridge(ui: &mut egui::Ui, state: &mut MixerState, palette: &HarmoniqPalette) {
    let filter_text = state.channel_filter.to_lowercase();
    let filter = filter_text.trim();

    Frame::none()
        .fill(palette.mixer_strip_bg)
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for channel in state
                            .channels
                            .iter()
                            .filter(|ch| ch.visible && channel_matches(filter, ch))
                        {
                            let meter = &channel.meter;
                            ui.vertical(|ui| {
                                ui.centered_and_justified(|ui| {
                                    ui.label(
                                        RichText::new(channel.name.clone())
                                            .small()
                                            .color(palette.text_primary),
                                    );
                                });
                                ui.add(
                                    LevelMeter::new(palette)
                                        .with_levels(
                                            meter.peak_l,
                                            meter.peak_r,
                                            0.5 * (meter.rms_l + meter.rms_r),
                                        )
                                        .with_size(egui::vec2(24.0, 120.0))
                                        .with_clip(meter.clip_l, meter.clip_r)
                                        .interactive(false),
                                );
                            });
                        }
                    });
                });
        });
}

fn channel_matches(filter: &str, channel: &Channel) -> bool {
    if filter.is_empty() {
        return true;
    }
    let name = channel.name.to_lowercase();
    let input = channel.input_bus.to_lowercase();
    let output = channel.output_bus.to_lowercase();
    name.contains(filter) || input.contains(filter) || output.contains(filter)
}

fn channel_area(
    ui: &mut egui::Ui,
    state: &mut MixerState,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
) {
    let metrics = StripMetrics::default();
    let filter_text = state.channel_filter.to_lowercase();
    let filter = filter_text.trim();

    egui::ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(16.0, 0.0);
            let mut reset_requests: Vec<ResetRequest> = Vec::new();
            let mut selection_request: Option<ChannelId> = None;
            let selected = state.selected;
            ui.horizontal_top(|ui| {
                for channel in state.channels.iter_mut() {
                    if !channel.visible {
                        continue;
                    }
                    if !channel_matches(filter, channel) {
                        continue;
                    }
                    let events = channel_strip(
                        ui,
                        state,
                        channel,
                        callbacks,
                        palette,
                        selected == Some(channel.id),
                        &metrics,
                    );
                    if let Some(reset) = events.reset {
                        reset_requests.push(reset);
                    }
                    if events.select {
                        selection_request = Some(channel.id);
                    }
                }
            });

            if let Some(id) = selection_request {
                state.selected = Some(id);
            }

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
}

struct StripEvents {
    reset: Option<ResetRequest>,
    select: bool,
}

fn channel_strip(
    ui: &mut egui::Ui,
    state: &MixerState,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    is_selected: bool,
    metrics: &StripMetrics,
) -> StripEvents {
    let mut reset_request = None;
    let mut select = false;

    let strip_response = Frame::none()
        .fill(strip_fill(palette, is_selected, channel))
        .stroke(Stroke::new(1.0, palette.mixer_strip_border))
        .rounding(Rounding::same(14.0))
        .inner_margin(Margin::symmetric(16.0, 14.0))
        .show(ui, |ui| {
            ui.set_width(metrics.strip_w);
            ui.spacing_mut().item_spacing = egui::vec2(10.0, metrics.section_spacing);

            strip_header(ui, channel, palette);

            if state.rack_visibility.input {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.input_expanded,
                    "Input Routing",
                    |ui| {
                        input_section(ui, channel, palette);
                    },
                );
            }

            if state.rack_visibility.pre {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.pre_expanded,
                    "Pre",
                    |ui| {
                        pre_section(ui, channel, palette);
                    },
                );
            }

            if state.rack_visibility.strip {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.strip_expanded,
                    "Channel Strip",
                    |ui| channel_strip_section(ui, channel, palette),
                );
            }

            if state.rack_visibility.eq {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.eq_expanded,
                    "EQ",
                    |ui| eq_section(ui, channel, palette),
                );
            }

            if state.rack_visibility.inserts {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.inserts_expanded,
                    "Inserts",
                    |ui| inserts_panel(ui, channel, callbacks, palette, metrics),
                );
            }

            if state.rack_visibility.sends {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.sends_expanded,
                    "Sends",
                    |ui| sends_section(ui, channel, callbacks, palette, metrics),
                );
            }

            if state.rack_visibility.cues {
                rack_section(
                    ui,
                    palette,
                    &mut channel.rack_state.cues_expanded,
                    "Cue Sends",
                    |ui| cue_section(ui, channel, palette, metrics),
                );
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

            ui.add_space(6.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                let pan_response = ui.add(
                    Knob::new(&mut channel.pan, -1.0, 1.0, 0.0, "Pan", palette)
                        .with_diameter(metrics.pan_knob_diameter),
                );
                if pan_response.changed() {
                    (callbacks.set_gain_pan)(channel.id, channel.gain_db, channel.pan);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                    let mute_response = ui
                        .add(
                            StateToggleButton::new(&mut channel.mute, "M", palette)
                                .with_width(40.0),
                        )
                        .on_hover_text("Mute");
                    if mute_response.changed() {
                        (callbacks.set_mute)(channel.id, channel.mute);
                    }
                    let solo_response = ui
                        .add(
                            StateToggleButton::new(&mut channel.solo, "S", palette)
                                .with_width(40.0),
                        )
                        .on_hover_text("Solo");
                    if solo_response.changed() {
                        (callbacks.set_solo)(channel.id, channel.solo);
                    }
                });
            });
        })
        .response;

    if strip_response.clicked() {
        select = true;
    }

    strip_response.context_menu(|ui| {
        if ui.button("Add Insert…").clicked() {
            (callbacks.open_insert_browser)(channel.id, None);
            ui.close_menu();
        }
        if ui.button("Toggle Analyzer").clicked() {
            channel.eq.analyzer_enabled = !channel.eq.analyzer_enabled;
            ui.close_menu();
        }
    });

    StripEvents {
        reset: reset_request,
        select,
    }
}

fn strip_fill(palette: &HarmoniqPalette, is_selected: bool, channel: &Channel) -> Color32 {
    if channel.record_enable {
        palette.mixer_strip_solo
    } else if channel.solo {
        palette.mixer_strip_solo
    } else if channel.mute {
        palette.mixer_strip_muted
    } else if is_selected {
        palette.mixer_strip_selected
    } else {
        palette.mixer_strip_bg
    }
}

fn strip_header(ui: &mut egui::Ui, channel: &mut Channel, palette: &HarmoniqPalette) {
    Frame::none()
        .fill(palette.toolbar_highlight)
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::symmetric(10.0, 8.0))
        .stroke(Stroke::new(1.0, palette.toolbar_outline))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let color =
                        Color32::from_rgb(channel.color[0], channel.color[1], channel.color[2]);
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, color);
                    ui.add_space(6.0);
                    let mut name = channel.name.clone();
                    let response = egui::TextEdit::singleline(&mut name)
                        .desired_width(ui.available_width())
                        .font(TextStyle::Monospace);
                    if ui.add(response).lost_focus() {
                        channel.name = name;
                    }
                });

                ui.add_space(6.0);
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
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                    ui.add(
                        StateToggleButton::new(&mut channel.record_enable, "Rec", palette)
                            .with_width(52.0),
                    );
                    ui.add(
                        StateToggleButton::new(&mut channel.monitor_enable, "Mon", palette)
                            .with_width(52.0),
                    );
                    ui.add(
                        StateToggleButton::new(&mut channel.phase_invert, "Ø", palette)
                            .with_width(40.0),
                    );
                    ComboBox::from_label("")
                        .selected_text(channel.automation.label())
                        .show_ui(ui, |ui| {
                            for mode in [
                                AutomationMode::Off,
                                AutomationMode::Read,
                                AutomationMode::Touch,
                                AutomationMode::Latch,
                                AutomationMode::Write,
                            ] {
                                ui.selectable_value(&mut channel.automation, mode, mode.label());
                            }
                        });
                });
            });
        });
}

fn rack_section(
    ui: &mut egui::Ui,
    palette: &HarmoniqPalette,
    expanded: &mut bool,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) {
    let header_response = Frame::none()
        .fill(palette.mixer_slot_bg)
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0, palette.mixer_slot_border))
        .inner_margin(Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            let icon = if *expanded { "▾" } else { "▸" };
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).strong().color(palette.text_primary));
                ui.label(RichText::new(title).strong().color(palette.text_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Bypass").clicked() {
                        *expanded = true;
                    }
                });
            });
        })
        .response;

    if header_response.clicked() {
        *expanded = !*expanded;
    }

    if *expanded {
        Frame::none()
            .fill(palette.mixer_strip_bg)
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, content);
    }
}

fn input_section(ui: &mut egui::Ui, channel: &mut Channel, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.label(RichText::new("Input").small().color(palette.text_muted));
        ui.horizontal(|ui| {
            ui.label(RichText::new(channel.input_bus.clone()).color(palette.text_primary));
            if ui.small_button("Change").clicked() {
                // placeholder for routing popup
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Output").small().color(palette.text_muted));
        ui.horizontal(|ui| {
            ui.label(RichText::new(channel.output_bus.clone()).color(palette.text_primary));
            if ui.small_button("Change").clicked() {}
        });
    });
}

fn pre_section(ui: &mut egui::Ui, channel: &mut Channel, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pre Gain").small().color(palette.text_muted));
            let mut gain = channel.pre_gain_db;
            let resp =
                ui.add(Knob::new(&mut gain, -24.0, 24.0, 0.0, "dB", palette).with_diameter(46.0));
            if resp.changed() {
                channel.pre_gain_db = gain;
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Low Cut").small().color(palette.text_muted));
            let mut freq = channel.low_cut_hz;
            let resp =
                ui.add(Knob::new(&mut freq, 20.0, 1000.0, 20.0, "Hz", palette).with_diameter(46.0));
            if resp.changed() {
                channel.low_cut_hz = freq;
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("High Cut").small().color(palette.text_muted));
            let mut freq = channel.high_cut_hz;
            let resp = ui.add(
                Knob::new(&mut freq, 2000.0, 20000.0, 20000.0, "Hz", palette).with_diameter(46.0),
            );
            if resp.changed() {
                channel.high_cut_hz = freq;
            }
        });
    });
}

fn channel_strip_section(ui: &mut egui::Ui, channel: &mut Channel, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.label(RichText::new("Drive").small().color(palette.text_muted));
        let mut drive = channel.strip_modules.drive;
        if ui
            .add(Knob::new(&mut drive, 0.0, 1.0, 0.0, "", palette).with_diameter(42.0))
            .changed()
        {
            channel.strip_modules.drive = drive;
        }

        ui.add_space(6.0);
        ui.checkbox(&mut channel.strip_modules.gate_enabled, "Gate");
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Compressor")
                    .small()
                    .color(palette.text_muted),
            );
            let mut comp = channel.strip_modules.compressor;
            if ui
                .add(Knob::new(&mut comp, 0.0, 1.0, 0.0, "", palette).with_diameter(42.0))
                .changed()
            {
                channel.strip_modules.compressor = comp;
            }
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Saturation")
                    .small()
                    .color(palette.text_muted),
            );
            let mut sat = channel.strip_modules.saturation;
            if ui
                .add(Knob::new(&mut sat, 0.0, 1.0, 0.0, "", palette).with_diameter(42.0))
                .changed()
            {
                channel.strip_modules.saturation = sat;
            }
        });
        ui.checkbox(&mut channel.strip_modules.limiter_enabled, "Limiter");
    });
}

fn eq_section(ui: &mut egui::Ui, channel: &mut Channel, palette: &HarmoniqPalette) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.checkbox(&mut channel.eq.enabled, "Enable");
            ui.checkbox(&mut channel.eq.analyzer_enabled, "Analyzer");
        });
        ui.add_space(6.0);
        eq_curve(ui, &channel.eq, palette);
        ui.add_space(10.0);
        for (index, band) in channel.eq.bands.iter_mut().enumerate() {
            eq_band_row(ui, index, band, palette);
            ui.add_space(6.0);
        }
    });
}

fn eq_curve(ui: &mut egui::Ui, eq: &ChannelEq, palette: &HarmoniqPalette) {
    let size = Vec2::new(148.0, 96.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, palette.mixer_slot_bg);
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, palette.mixer_slot_border));

    let freqs = [
        20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
    ];
    for freq in freqs {
        let t = (freq.log10() - 1.3010) / (4.3010 - 1.3010);
        let x = rect.left() + rect.width() * t.clamp(0.0, 1.0);
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom()),
                Pos2::new(x, rect.bottom() - 6.0),
            ],
            Stroke::new(1.0, palette.toolbar_outline),
        );
    }

    let mut points: Vec<Pos2> = Vec::new();
    let steps = 120;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let freq = 20.0 * (20000.0 / 20.0).powf(t);
        let gain = eq_gain(eq, freq);
        let norm = ((gain + 18.0) / 36.0).clamp(0.0, 1.0);
        let x = egui::lerp(rect.left()..=rect.right(), t);
        let y = rect.bottom() - norm * rect.height();
        points.push(Pos2::new(x, y));
    }
    let stroke = if eq.enabled {
        Stroke::new(2.0, palette.accent)
    } else {
        Stroke::new(1.0, palette.text_muted)
    };
    painter.add(Shape::line(points, stroke));
}

fn eq_gain(eq: &ChannelEq, freq: f32) -> f32 {
    if !eq.enabled {
        return 0.0;
    }
    eq.bands
        .iter()
        .filter(|band| band.enabled)
        .map(|band| band_gain(band, freq))
        .sum()
}

fn band_gain(band: &EqBand, freq: f32) -> f32 {
    let freq = freq.max(1.0);
    let fc = band.frequency_hz.max(20.0);
    match band.kind {
        EqFilterKind::LowCut => {
            if freq < fc {
                let ratio = 1.0 - (freq / fc).clamp(0.0, 1.0);
                -24.0 * ratio.powf(1.2)
            } else {
                0.0
            }
        }
        EqFilterKind::LowShelf => {
            let ratio = (freq / fc).clamp(0.0, 2.0);
            band.gain_db * (ratio / 2.0).powf(0.6)
        }
        EqFilterKind::Peak => {
            let q = band.q.max(0.1);
            let dist = ((freq / fc).ln()).abs();
            band.gain_db * (-dist * q).exp()
        }
        EqFilterKind::HighShelf => {
            let ratio = (freq / fc).clamp(0.0, 2.0);
            band.gain_db * (ratio / 2.0).powf(0.6)
        }
        EqFilterKind::HighCut => {
            if freq > fc {
                let ratio = (freq / fc - 1.0).clamp(0.0, 1.0);
                -24.0 * ratio.powf(1.2)
            } else {
                0.0
            }
        }
    }
}

fn eq_band_row(ui: &mut egui::Ui, index: usize, band: &mut EqBand, palette: &HarmoniqPalette) {
    Frame::none()
        .fill(palette.toolbar_highlight)
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut band.enabled, format!("Band {}", index + 1));
                    ComboBox::from_label("")
                        .selected_text(band.kind.label())
                        .show_ui(ui, |ui| {
                            for kind in [
                                EqFilterKind::LowCut,
                                EqFilterKind::LowShelf,
                                EqFilterKind::Peak,
                                EqFilterKind::HighShelf,
                                EqFilterKind::HighCut,
                            ] {
                                ui.selectable_value(&mut band.kind, kind, kind.label());
                            }
                        });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                    let mut freq = band.frequency_hz;
                    if ui
                        .add(
                            Knob::new(&mut freq, 20.0, 20000.0, 1000.0, "Hz", palette)
                                .with_diameter(40.0),
                        )
                        .changed()
                    {
                        band.frequency_hz = freq;
                    }
                    let mut gain = band.gain_db;
                    if ui
                        .add(
                            Knob::new(&mut gain, -24.0, 24.0, 0.0, "dB", palette)
                                .with_diameter(40.0),
                        )
                        .changed()
                    {
                        band.gain_db = gain;
                    }
                    let mut q = band.q;
                    if ui
                        .add(Knob::new(&mut q, 0.2, 10.0, 1.0, "Q", palette).with_diameter(40.0))
                        .changed()
                    {
                        band.q = q;
                    }
                });
            });
        });
}

fn sends_section(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut crate::MixerCallbacks,
    palette: &HarmoniqPalette,
    metrics: &StripMetrics,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        if channel.sends.is_empty() {
            ui.label(
                RichText::new("No sends configured.")
                    .small()
                    .color(palette.text_muted),
            );
        }
        for send in &mut channel.sends {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                ui.label(
                    RichText::new(format!("{}", (b'A' + send.id) as char))
                        .color(palette.text_primary),
                );
                let mut level = send.level;
                let response = ui.add(
                    Knob::new(&mut level, 0.0, 1.0, 0.0, "", palette)
                        .with_diameter(metrics.send_knob_diameter),
                );
                if response.changed() {
                    send.level = level;
                    (callbacks.configure_send)(channel.id, send.id, send.level);
                }
            });
        }

        if ui.small_button("Add Send").clicked() {
            let id = channel.sends.last().map(|s| s.id + 1).unwrap_or(0);
            channel
                .sends
                .push(crate::state::SendSlot { id, level: 0.0 });
        }
    });
}

fn cue_section(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    palette: &HarmoniqPalette,
    metrics: &StripMetrics,
) {
    ui.vertical(|ui| {
        if channel.cue_sends.is_empty() {
            ui.label(
                RichText::new("No cue buses configured.")
                    .small()
                    .color(palette.text_muted),
            );
        }
        for cue in &mut channel.cue_sends {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                ui.checkbox(&mut cue.enabled, &cue.name);
                ui.checkbox(&mut cue.pre_fader, "Pre");
            });
            let mut level = cue.level;
            if ui
                .add(
                    Knob::new(&mut level, 0.0, 1.0, 0.0, "", palette)
                        .with_diameter(metrics.cue_knob_diameter),
                )
                .changed()
            {
                cue.level = level;
            }
        }
        if ui.small_button("Add Cue").clicked() {
            let id = channel.cue_sends.last().map(|s| s.id + 1).unwrap_or(0);
            channel.cue_sends.push(CueSend {
                id,
                name: format!("Cue {}", id + 1),
                level: 0.0,
                enabled: false,
                pre_fader: false,
            });
        }
    });
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
            let frame = Frame::none()
                .fill(slot_fill)
                .stroke(Stroke::new(1.0, palette.mixer_slot_border))
                .rounding(Rounding::same(8.0))
                .inner_margin(Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                        let handle = ui
                            .add(egui::Label::new("≡").sense(Sense::drag()))
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
        .default_size(egui::vec2(780.0, 460.0))
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Level 0..1. Click a cell to toggle; drag vertically to adjust.")
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
                            let (rect, response) = grid_ui.allocate_exact_size(
                                egui::vec2(96.0, 32.0),
                                Sense::click_and_drag(),
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
                                Align2::CENTER_CENTER,
                                format!("{current:.2}"),
                                TextStyle::Small.resolve(grid_ui.style()),
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
