pub mod model;
mod widgets;

pub use harmoniq_engine::mixer::rt_api;

use std::time::{Duration, Instant};

use egui::{self, vec2, Align, Color32, Layout, Margin, RichText, Sense};
use widgets::{GainFader, MeterDisplay, PanKnob};

use self::model::{ChannelView, MixerView, SendView};
use rt_api::{MixerBus, MixerMsg, MixerStateSnapshot, PanLaw, SEND_COUNT};

pub use model::{
    ChannelId, ChannelView as MixerChannelView, MixerIcon, MixerMetrics,
    MixerTheme as MixerThemeDefinition, MixerView as MixerViewModel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterBallistics {
    Digital,
    Ansi,
    Ebu,
}

impl MeterBallistics {
    fn label(self) -> &'static str {
        match self {
            MeterBallistics::Digital => "Digital",
            MeterBallistics::Ansi => "ANSI",
            MeterBallistics::Ebu => "EBU",
        }
    }
}

pub struct MixerPanel {
    pub view: MixerView,
    bus: MixerBus,
    pub visible: bool,
    pub show_routing_matrix: bool,
    meter_ballistics: MeterBallistics,
    peak_hold: bool,
    pan_law: PanLaw,
    last_snapshot: Option<MixerStateSnapshot>,
    last_meter_tick: Instant,
    cpu_usage: Vec<f32>,
}

impl MixerPanel {
    pub fn new(initial_channels: usize, bus: MixerBus) -> Self {
        MixerPanel {
            view: MixerView::new(initial_channels),
            bus,
            visible: true,
            show_routing_matrix: true,
            meter_ballistics: MeterBallistics::Digital,
            peak_hold: true,
            pan_law: PanLaw::default(),
            last_snapshot: None,
            last_meter_tick: Instant::now(),
            cpu_usage: Vec::new(),
        }
    }

    pub fn bus(&self) -> &MixerBus {
        &self.bus
    }

    pub fn bus_mut(&mut self) -> &mut MixerBus {
        &mut self.bus
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.handle_shortcuts(ctx);
        if !self.visible {
            return;
        }

        self.poll_snapshots();

        let theme = self.view.theme.clone();

        egui::TopBottomPanel::top("mixer_toolbar").show(ctx, |ui| {
            ui.set_height(38.0);
            self.toolbar_ui(ui);
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(theme.background))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(ui.available_height());
                self.channels_ui(ui);
                ui.add_space(8.0);
                if self.show_routing_matrix {
                    ui.separator();
                    self.routing_matrix_ui(ui);
                }
            });
    }

    fn toolbar_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(
                RichText::new("Mixer")
                    .size(18.0)
                    .color(self.view.theme.text_primary),
            );

            egui::ComboBox::from_id_source("meter_ballistics")
                .selected_text(self.meter_ballistics.label())
                .show_ui(ui, |ui| {
                    for mode in [
                        MeterBallistics::Digital,
                        MeterBallistics::Ansi,
                        MeterBallistics::Ebu,
                    ] {
                        ui.selectable_value(&mut self.meter_ballistics, mode, mode.label());
                    }
                });

            ui.checkbox(&mut self.peak_hold, "Peak Hold");

            egui::ComboBox::from_id_source("pan_law")
                .selected_text(match self.pan_law {
                    PanLaw::Linear => "Linear",
                    PanLaw::Minus3dB => "-3 dB",
                    PanLaw::Minus4Point5dB => "-4.5 dB",
                })
                .show_ui(ui, |ui| {
                    for law in [PanLaw::Linear, PanLaw::Minus3dB, PanLaw::Minus4Point5dB] {
                        let label = match law {
                            PanLaw::Linear => "Linear",
                            PanLaw::Minus3dB => "-3 dB",
                            PanLaw::Minus4Point5dB => "-4.5 dB",
                        };
                        if ui.selectable_label(self.pan_law == law, label).clicked() {
                            self.pan_law = law;
                            let _ = self.bus.tx.try_send(MixerMsg::SetPanLaw { law });
                        }
                    }
                });

            ui.separator();
            ui.label(RichText::new("Oversampling: Preview").color(self.view.theme.text_secondary));

            ui.separator();
            ui.label(RichText::new("CPU").color(self.view.theme.text_secondary));
            if self.cpu_usage.is_empty() {
                ui.label("n/a");
            } else {
                for (idx, load) in self.cpu_usage.iter().enumerate() {
                    let text = format!("Core {}: {:>4.1}%", idx + 1, load * 100.0);
                    ui.label(text);
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Reset Peaks").clicked() {
                    let _ = self.bus.tx.try_send(MixerMsg::ResetPeaks);
                }
            });
        });
    }

    fn channels_ui(&mut self, ui: &mut egui::Ui) {
        let total_strips = self.view.channels.len() + SEND_COUNT + 1;
        let width = self.view.theme.metrics.channel_width;
        let height = self.view.theme.metrics.channel_height;

        egui::ScrollArea::horizontal()
            .id_source("mixer_channel_scroll")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let total_width = width * total_strips as f32;
                let (canvas_rect, _) =
                    ui.allocate_exact_size(vec2(total_width, height), Sense::hover());
                let mut child = ui.child_ui(canvas_rect, Layout::left_to_right(Align::Min));
                child.spacing_mut().item_spacing.x = 6.0;

                let mut start = (viewport.min.x / width).floor() as isize - 1;
                let mut end = (viewport.max.x / width).ceil() as isize + 1;
                start = start.clamp(0, total_strips as isize);
                end = end.clamp(0, total_strips as isize);

                if start > 0 {
                    child.add_space(width * start as f32);
                }

                for idx in start as usize..end as usize {
                    let kind = self.strip_kind(idx);
                    self.draw_strip(&mut child, kind, width, height);
                }

                if end as usize <= total_strips {
                    let trailing = total_strips.saturating_sub(end as usize);
                    if trailing > 0 {
                        child.add_space(width * trailing as f32);
                    }
                }
            });
    }

    fn strip_kind(&self, index: usize) -> StripKind {
        if index < self.view.channels.len() {
            StripKind::Channel(index)
        } else if index < self.view.channels.len() + SEND_COUNT {
            StripKind::Send(index - self.view.channels.len())
        } else {
            StripKind::Master
        }
    }

    fn draw_strip(&mut self, ui: &mut egui::Ui, kind: StripKind, width: f32, height: f32) {
        match kind {
            StripKind::Channel(idx) => {
                if idx < self.view.channels.len() {
                    let mut channel = self.view.channels[idx].clone();
                    let clicked = self.draw_channel_strip(ui, idx, &mut channel, width, height);
                    self.view.channels[idx] = channel;
                    if clicked {
                        self.view.selection = Some(idx);
                    }
                }
            }
            StripKind::Send(idx) => {
                if idx < self.view.sends.len() {
                    let mut send = self.view.sends[idx].clone();
                    self.draw_send_strip(ui, idx, &mut send, width, height);
                    self.view.sends[idx] = send;
                }
            }
            StripKind::Master => {
                let master_index = self.view.channels.len() + SEND_COUNT;
                let mut master = self.view.master.clone();
                self.draw_master_strip(ui, master_index, &mut master, width, height);
                self.view.master = master;
            }
        }
    }

    fn draw_channel_strip(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        channel: &mut ChannelView,
        width: f32,
        height: f32,
    ) -> bool {
        let selected = self.view.selection == Some(index);
        let frame = egui::Frame::none()
            .fill(if selected {
                self.view.theme.strip_selected
            } else {
                self.view.theme.strip_bg
            })
            .stroke(egui::Stroke::new(1.0, self.view.theme.strip_border))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 8.0));

        let inner = frame.show(ui, |ui| {
            ui.set_width(width - 6.0);
            ui.set_height(height - 12.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let label = format!("{:02}", index + 1);
                    ui.label(RichText::new(label).color(self.view.theme.text_secondary));
                    let _ = ui.add(
                        egui::TextEdit::singleline(&mut channel.name)
                            .desired_width(width - 40.0)
                            .clip_text(false),
                    );
                });

                ui.add_space(4.0);
                self.draw_meters(ui, channel);

                ui.add_space(6.0);
                let fader_resp = ui.add(GainFader::new(
                    &mut channel.gain_db,
                    self.view.theme.clone(),
                ));
                if fader_resp.changed() {
                    let _ = self.bus.tx.try_send(MixerMsg::SetGain {
                        ch: index,
                        db: channel.gain_db,
                    });
                }

                ui.add_space(6.0);
                let pan_resp = ui.add(PanKnob::new(&mut channel.pan, self.pan_law));
                if pan_resp.changed() {
                    let _ = self.bus.tx.try_send(MixerMsg::SetPan {
                        ch: index,
                        pan: channel.pan,
                    });
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.toggle_value(&mut channel.mute, "M").clicked() {
                        let _ = self.bus.tx.try_send(MixerMsg::SetMute {
                            ch: index,
                            on: channel.mute,
                        });
                    }
                    if ui.toggle_value(&mut channel.solo, "S").clicked() {
                        let _ = self.bus.tx.try_send(MixerMsg::SetSolo {
                            ch: index,
                            on: channel.solo,
                        });
                    }
                    if ui.toggle_value(&mut channel.record_arm, "R").clicked() {
                        // record arm is UI-only stub for now
                    }
                });

                ui.horizontal(|ui| {
                    if ui.small_button("Ø").on_hover_text("Phase Invert").clicked() {
                        channel.phase_invert = !channel.phase_invert;
                        let _ = self.bus.tx.try_send(MixerMsg::SetPhaseInvert {
                            ch: index,
                            on: channel.phase_invert,
                        });
                    }
                    if ui
                        .small_button("Mono")
                        .on_hover_text("Mono Summing")
                        .clicked()
                    {
                        channel.mono = !channel.mono;
                        let _ = self.bus.tx.try_send(MixerMsg::SetMono {
                            ch: index,
                            on: channel.mono,
                        });
                    }
                    if ui
                        .small_button("Link")
                        .on_hover_text("Stereo Link")
                        .clicked()
                    {
                        channel.stereo_link = !channel.stereo_link;
                        let _ = self.bus.tx.try_send(MixerMsg::SetStereoLink {
                            ch: index,
                            on: channel.stereo_link,
                        });
                    }
                });

                ui.separator();
                ui.label(RichText::new("FX Inserts").color(self.view.theme.text_secondary));
                self.draw_insert_rack(ui, index, channel);

                ui.add_space(6.0);
                let latency = format!("Latency: {} smp", channel.latency_samples);
                ui.label(RichText::new(latency).color(self.view.theme.text_secondary));
            });
        });
        let response = inner.response;
        response.clicked()
    }
    fn draw_send_strip(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        send: &mut SendView,
        width: f32,
        height: f32,
    ) {
        let frame = egui::Frame::none()
            .fill(self.view.theme.strip_bg)
            .stroke(egui::Stroke::new(1.0, self.view.theme.strip_border))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 8.0));
        frame.show(ui, |ui| {
            ui.set_width(width - 6.0);
            ui.set_height(height - 12.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("Send {}", index + 1))
                        .color(self.view.theme.text_primary),
                );
                ui.add_space(4.0);
                self.draw_meters(ui, &send.channel);
                ui.add_space(6.0);
                if ui
                    .add(GainFader::new(
                        &mut send.send_gain_db,
                        self.view.theme.clone(),
                    ))
                    .changed()
                {
                    let dst = self.view.channels.len() + index;
                    let _ = self.bus.tx.try_send(MixerMsg::SetSendGain {
                        src: dst,
                        dst: self.view.channels.len() + SEND_COUNT,
                        db: send.send_gain_db,
                    });
                }
                if ui.checkbox(&mut send.pre_fader, "Pre").clicked() {
                    // send pre/post toggle is UI only for now
                }
                let latency = format!("Latency: {} smp", send.channel.latency_samples);
                ui.label(RichText::new(latency).color(self.view.theme.text_secondary));
            });
        });
    }

    fn draw_master_strip(
        &mut self,
        ui: &mut egui::Ui,
        master_index: usize,
        channel: &mut ChannelView,
        width: f32,
        height: f32,
    ) {
        let frame = egui::Frame::none()
            .fill(self.view.theme.strip_bg)
            .stroke(egui::Stroke::new(1.0, self.view.theme.strip_border))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 8.0));
        frame.show(ui, |ui| {
            ui.set_width(width - 6.0);
            ui.set_height(height - 12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Master").color(self.view.theme.text_primary));
                ui.add_space(4.0);
                self.draw_meters(ui, channel);
                let lufs = channel.meter_rms_l.max(channel.meter_rms_r);
                ui.label(
                    RichText::new(format!("LUFS (stub): {:>4.1} dB", lufs))
                        .color(self.view.theme.text_secondary),
                );
                ui.add_space(6.0);

                if ui
                    .add(GainFader::new(
                        &mut channel.gain_db,
                        self.view.theme.clone(),
                    ))
                    .changed()
                {
                    let _ = self.bus.tx.try_send(MixerMsg::SetGain {
                        ch: master_index,
                        db: channel.gain_db,
                    });
                }
                ui.add_space(6.0);
                if ui
                    .add(PanKnob::new(&mut channel.pan, self.pan_law))
                    .changed()
                {
                    let _ = self.bus.tx.try_send(MixerMsg::SetPan {
                        ch: master_index,
                        pan: channel.pan,
                    });
                }

                ui.horizontal(|ui| {
                    if ui.toggle_value(&mut channel.mute, "M").clicked() {
                        let _ = self.bus.tx.try_send(MixerMsg::SetMute {
                            ch: master_index,
                            on: channel.mute,
                        });
                    }
                    if ui.toggle_value(&mut channel.solo, "S").clicked() {
                        let _ = self.bus.tx.try_send(MixerMsg::SetSolo {
                            ch: master_index,
                            on: channel.solo,
                        });
                    }
                });

                ui.horizontal(|ui| {
                    if ui.small_button("Ø").clicked() {
                        channel.phase_invert = !channel.phase_invert;
                        let _ = self.bus.tx.try_send(MixerMsg::SetPhaseInvert {
                            ch: master_index,
                            on: channel.phase_invert,
                        });
                    }
                    if ui.small_button("Mono").clicked() {
                        channel.mono = !channel.mono;
                        let _ = self.bus.tx.try_send(MixerMsg::SetMono {
                            ch: master_index,
                            on: channel.mono,
                        });
                    }
                });

                ui.separator();
                ui.label(RichText::new("Latency").color(self.view.theme.text_secondary));
                ui.label(
                    RichText::new(format!("{} smp", channel.latency_samples))
                        .color(self.view.theme.text_secondary),
                );
            });
        });
    }

    fn draw_meters(&self, ui: &mut egui::Ui, channel: &ChannelView) {
        let meter = MeterDisplay::new(
            channel.meter_peak_l,
            channel.meter_peak_r,
            channel.meter_rms_l,
            channel.meter_rms_r,
            self.view.theme.clone(),
        );
        ui.add(meter);
    }

    fn draw_insert_rack(
        &mut self,
        ui: &mut egui::Ui,
        channel_index: usize,
        channel: &mut ChannelView,
    ) {
        let available_width = ui.available_width();
        let mut swap_request: Option<(usize, usize)> = None;
        for (slot_index, slot) in channel.inserts.iter_mut().enumerate() {
            let frame = egui::Frame::none()
                .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 20))
                .stroke(egui::Stroke::new(1.0, self.view.theme.strip_border))
                .inner_margin(Margin::symmetric(4.0, 2.0));
            let response = frame.show(ui, |ui| {
                ui.set_width(available_width - 8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{:02}", slot_index + 1))
                            .color(self.view.theme.text_secondary),
                    );
                    let label = slot
                        .plugin_id
                        .map(|id| format!("Plugin #{id}"))
                        .unwrap_or_else(|| "(empty)".to_string());
                    let selectable = egui::SelectableLabel::new(
                        self.view.selected_insert == Some((channel_index, slot_index)),
                        label,
                    );
                    if ui.add(selectable).clicked() {
                        self.view.selected_insert = Some((channel_index, slot_index));
                    }
                    if ui.toggle_value(&mut slot.bypass, "Byp").clicked() {
                        let _ = self.bus.tx.try_send(MixerMsg::SetInsertBypass {
                            ch: channel_index,
                            slot: slot_index,
                            on: slot.bypass,
                        });
                    }
                    if ui.toggle_value(&mut slot.post_fader, "Post").clicked() {
                        let _ = self.bus.tx.try_send(MixerMsg::SetInsertPostFader {
                            ch: channel_index,
                            slot: slot_index,
                            post: slot.post_fader,
                        });
                    }
                    let color = if slot.bypass {
                        Color32::from_rgb(120, 120, 120)
                    } else {
                        self.view.theme.accent
                    };
                    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 4.0, color);
                });
            });
            let response = response.response;
            if response.drag_started() {
                self.view.drag_insert = Some(self::model::InsertDragState {
                    channel: channel_index,
                    slot: slot_index,
                });
            }
            if response.drag_stopped() {
                if let Some(drag) = self.view.drag_insert.take() {
                    if drag.channel == channel_index && drag.slot != slot_index {
                        swap_request = Some((drag.slot, slot_index));
                    }
                }
            }
        }

        if let Some((from, to)) = swap_request {
            channel.inserts.swap(from, to);
        }
    }

    fn routing_matrix_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Routing")
                .size(16.0)
                .color(self.view.theme.text_primary),
        );
        ui.add_space(4.0);
        let sources = self.view.channels.len();
        let destinations = self.view.channels.len() + SEND_COUNT + 1;
        self.view
            .routing
            .ensure_dimensions(sources + SEND_COUNT, destinations);

        egui::ScrollArea::both()
            .id_source("routing_matrix_scroll")
            .show(ui, |ui| {
                egui::Grid::new("routing_matrix_grid")
                    .spacing(vec2(6.0, 4.0))
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("");
                        for dst in 0..destinations {
                            ui.label(self.routing_destination_label(dst));
                        }
                        ui.end_row();

                        for src in 0..(sources + SEND_COUNT) {
                            ui.label(self.routing_source_label(src));
                            for dst in 0..destinations {
                                if src == dst {
                                    ui.label("—");
                                    continue;
                                }
                                let mut cell = self
                                    .view
                                    .routing
                                    .route(src, dst)
                                    .cloned()
                                    .unwrap_or_default();
                                let button = egui::SelectableLabel::new(cell.enabled, "");
                                let mut response = ui.add_sized(vec2(18.0, 18.0), button);
                                if response.hovered() {
                                    response = response.on_hover_text(format!(
                                        "Send: {:.1} dB",
                                        cell.send_gain_db
                                    ));
                                    let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                                    if scroll.abs() > f32::EPSILON {
                                        cell.send_gain_db =
                                            (cell.send_gain_db + scroll * 0.1).clamp(-60.0, 12.0);
                                        let _ = self.bus.tx.try_send(MixerMsg::SetSendGain {
                                            src,
                                            dst,
                                            db: cell.send_gain_db,
                                        });
                                    }
                                }
                                if response.clicked() {
                                    if !cell.enabled {
                                        if !self.route_creates_cycle(src, dst) {
                                            cell.enabled = true;
                                            let _ = self.bus.tx.try_send(MixerMsg::SetRoute {
                                                src,
                                                dst,
                                                on: true,
                                            });
                                        }
                                    } else {
                                        cell.enabled = false;
                                        let _ = self.bus.tx.try_send(MixerMsg::SetRoute {
                                            src,
                                            dst,
                                            on: false,
                                        });
                                    }
                                }
                                if let Some(route) = self.view.routing.route_mut(src, dst) {
                                    *route = cell;
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    fn routing_source_label(&self, index: usize) -> String {
        if index < self.view.channels.len() {
            format!("Ch {:02}", index + 1)
        } else {
            format!("Send {}", index - self.view.channels.len() + 1)
        }
    }

    fn routing_destination_label(&self, index: usize) -> String {
        if index < self.view.channels.len() {
            format!("Ch {:02}", index + 1)
        } else if index < self.view.channels.len() + SEND_COUNT {
            format!("Send {}", index - self.view.channels.len() + 1)
        } else {
            "Master".to_string()
        }
    }

    fn route_creates_cycle(&self, src: usize, dst: usize) -> bool {
        let total = self.view.channels.len() + SEND_COUNT + 1;
        let mut visited = vec![false; total];
        self.dfs_has_path(dst, src, &mut visited)
    }

    fn dfs_has_path(&self, current: usize, target: usize, visited: &mut [bool]) -> bool {
        if current == target {
            return true;
        }
        if visited[current] {
            return false;
        }
        visited[current] = true;
        if let Some(row) = self.view.routing.routes.get(current) {
            for (dst, cell) in row.iter().enumerate() {
                if cell.enabled && self.dfs_has_path(dst, target, visited) {
                    return true;
                }
            }
        }
        false
    }

    fn poll_snapshots(&mut self) {
        let mut latest = None;
        while let Ok(snapshot) = self.bus.rx_snap.try_recv() {
            latest = Some(snapshot);
        }
        if let Some(snapshot) = latest {
            self.cpu_usage = snapshot.cpu_load_per_core.clone();
            self.view.apply_snapshot(&snapshot);
            self.last_snapshot = Some(snapshot);
            self.last_meter_tick = Instant::now();
        } else if self.last_meter_tick.elapsed() > Duration::from_millis(33) {
            if let Some(snapshot) = self.last_snapshot.clone() {
                self.view.apply_snapshot(&snapshot);
            }
            self.last_meter_tick = Instant::now();
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input(|input| {
            if input.key_pressed(egui::Key::F9) {
                self.visible = !self.visible;
            }
        });

        if !self.visible {
            return;
        }

        let mut reset_peaks = false;
        let mut toggle_mute = false;
        let mut toggle_solo = false;
        let mut select_delta: isize = 0;
        let mut delete_slot = false;

        ctx.input(|input| {
            if input.key_pressed(egui::Key::R) {
                reset_peaks = true;
            }
            if input.key_pressed(egui::Key::M) {
                toggle_mute = true;
            }
            if input.key_pressed(egui::Key::S) {
                toggle_solo = true;
            }
            if input.key_pressed(egui::Key::Delete) {
                delete_slot = true;
            }
            if input.modifiers.command || input.modifiers.ctrl {
                // ctrl / cmd
                if input.key_pressed(egui::Key::ArrowRight) {
                    select_delta = 1;
                } else if input.key_pressed(egui::Key::ArrowLeft) {
                    select_delta = -1;
                }
            }
        });

        if reset_peaks {
            let _ = self.bus.tx.try_send(MixerMsg::ResetPeaks);
        }

        if let Some(selected) = self.view.selection {
            if toggle_mute {
                if let Some(channel) = self.view.channels.get_mut(selected) {
                    channel.mute = !channel.mute;
                    let _ = self.bus.tx.try_send(MixerMsg::SetMute {
                        ch: selected,
                        on: channel.mute,
                    });
                }
            }
            if toggle_solo {
                if let Some(channel) = self.view.channels.get_mut(selected) {
                    channel.solo = !channel.solo;
                    let _ = self.bus.tx.try_send(MixerMsg::SetSolo {
                        ch: selected,
                        on: channel.solo,
                    });
                }
            }
            if delete_slot {
                if let Some((ch, slot)) = self.view.selected_insert {
                    if ch == selected {
                        if let Some(channel) = self.view.channels.get_mut(ch) {
                            channel.inserts[slot] = Default::default();
                        }
                    }
                }
            }
        }

        if select_delta != 0 {
            let total = self.view.channels.len();
            if total > 0 {
                let next = self
                    .view
                    .selection
                    .map(|idx| ((idx as isize + select_delta).rem_euclid(total as isize)) as usize)
                    .unwrap_or(0);
                self.view.selection = Some(next);
            }
        }
    }
}

enum StripKind {
    Channel(usize),
    Send(usize),
    Master,
}
