use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "mixer_api")]
use crossbeam_channel::Sender as MixerCommandSender;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Layout, Margin, RichText, Rounding, ScrollArea,
    Stroke, Ui, Vec2,
};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};
#[cfg(feature = "mixer_api")]
use harmoniq_engine::{GuiMeterReceiver, MixerCommand};
use harmoniq_mixer::state::{
    Channel, ChannelId, InsertSlot, Meter, MixerState, RoutingDelta, SendSlot,
};
use harmoniq_mixer::ui::{db_to_gain, gain_to_db};
use harmoniq_mixer::MixerCallbacks;
use harmoniq_ui::{
    widgets::{Fader, Knob, LevelMeter, StateToggleButton},
    HarmoniqPalette,
};
use tracing::{info, warn};

#[derive(Clone, Debug)]
struct StripSnapshot {
    index: usize,
    mute: bool,
    solo: bool,
    insert_bypass: Vec<bool>,
    send_levels_db: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResetRequest {
    Channel(ChannelId),
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionRequest {
    Select(ChannelId),
    Clear,
}

#[derive(Default)]
struct StripInteraction {
    reset: Option<ResetRequest>,
    selection: Option<SelectionRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MixerSkin {
    Compact,
    Classic,
}

impl MixerSkin {
    const ALL: [Self; 2] = [Self::Compact, Self::Classic];

    fn toggle(self) -> Self {
        match self {
            Self::Compact => Self::Classic,
            Self::Classic => Self::Compact,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Classic => "Classic",
        }
    }
}

#[derive(Clone, Copy)]
struct SlotSkin {
    empty_fill: Color32,
    active_fill: Color32,
    border: Color32,
    highlight: Color32,
    bypass_width: f32,
}

impl SlotSkin {
    fn classic(palette: &HarmoniqPalette) -> Self {
        Self {
            empty_fill: palette.mixer_slot_bg,
            active_fill: palette.mixer_slot_active,
            border: palette.mixer_slot_border,
            highlight: palette.accent_alt,
            bypass_width: 52.0,
        }
    }

    fn compact(palette: &HarmoniqPalette) -> Self {
        Self {
            empty_fill: Color32::from_rgb(38, 46, 54),
            active_fill: Color32::from_rgb(44, 60, 70),
            border: Color32::from_rgb(76, 98, 110),
            highlight: Color32::from_rgb(82, 214, 226),
            bypass_width: 46.0,
        }
    }
}

#[derive(Clone, Copy)]
struct CompactStripStyle {
    strip_width: f32,
    inner_margin: Margin,
    item_spacing: Vec2,
    section_spacing: f32,
    rounding: f32,
    meter_size: Vec2,
    meter_rounding: f32,
    meter_border: Color32,
    history_height: f32,
    history_rounding: f32,
    history_fill: Color32,
    history_line: Color32,
    history_glow: Color32,
    history_grid: Color32,
    fader_height: f32,
    fader_width: f32,
    knob_diameter: f32,
    send_knob_diameter: f32,
    toggle_width: f32,
    base_fill: Color32,
    border: Stroke,
    selected_border: Stroke,
    solo_fill: Color32,
    mute_fill: Color32,
    header_fill: Color32,
    header_text: Color32,
    label_primary: Color32,
    label_secondary: Color32,
    meter_bg: Color32,
    meter_peak: Color32,
    meter_rms: Color32,
    meter_hold: Color32,
    meter_tick: Color32,
    clip_on: Color32,
    clip_off: Color32,
    slot_colors: SlotSkin,
}

impl CompactStripStyle {
    fn new(palette: &HarmoniqPalette) -> Self {
        Self {
            strip_width: 112.0,
            inner_margin: Margin::symmetric(5.0, 6.0),
            item_spacing: Vec2::new(3.0, 4.0),
            section_spacing: 2.0,
            rounding: 7.0,
            meter_size: Vec2::new(14.0, 122.0),
            meter_rounding: 4.0,
            meter_border: Color32::from_rgb(54, 74, 82),
            history_height: 36.0,
            history_rounding: 5.0,
            history_fill: Color32::from_rgba_unmultiplied(28, 48, 58, 90),
            history_line: Color32::from_rgb(90, 208, 220),
            history_glow: Color32::from_rgba_unmultiplied(90, 208, 220, 60),
            history_grid: Color32::from_rgb(38, 52, 60),
            fader_height: 126.0,
            fader_width: 20.0,
            knob_diameter: 30.0,
            send_knob_diameter: 24.0,
            toggle_width: 24.0,
            base_fill: Color32::from_rgb(20, 26, 32),
            border: Stroke::new(1.0, Color32::from_rgb(46, 58, 66)),
            selected_border: Stroke::new(1.4, Color32::from_rgb(82, 214, 226)),
            solo_fill: Color32::from_rgb(28, 48, 54),
            mute_fill: Color32::from_rgb(34, 26, 38),
            header_fill: Color32::from_rgb(30, 38, 46),
            header_text: Color32::from_rgb(204, 228, 234),
            label_primary: Color32::from_rgb(198, 226, 232),
            label_secondary: Color32::from_rgb(126, 158, 168),
            meter_bg: Color32::from_rgb(14, 22, 28),
            meter_peak: Color32::from_rgb(64, 208, 220),
            meter_rms: Color32::from_rgb(28, 150, 170),
            meter_hold: Color32::from_rgb(210, 240, 246),
            meter_tick: Color32::from_rgb(56, 78, 88),
            clip_on: Color32::from_rgb(248, 96, 96),
            clip_off: Color32::from_rgb(70, 78, 84),
            slot_colors: SlotSkin::compact(palette),
        }
    }

    fn fill_for(&self, channel: &Channel, is_selected: bool) -> (Color32, Stroke) {
        let mut fill = if channel.solo {
            self.solo_fill
        } else if channel.mute {
            self.mute_fill
        } else {
            self.base_fill
        };
        if is_selected {
            fill = fill.gamma_multiply(1.12);
        }
        let stroke = if is_selected {
            self.selected_border
        } else {
            self.border
        };
        (fill, stroke)
    }
}

#[cfg(feature = "mixer_api")]
#[derive(Clone)]
pub struct MixerEngineBridge {
    sender: MixerCommandSender<MixerCommand>,
    meter_rx: GuiMeterReceiver,
}

#[cfg(feature = "mixer_api")]
impl MixerEngineBridge {
    pub fn new(sender: MixerCommandSender<MixerCommand>, meter_rx: GuiMeterReceiver) -> Self {
        Self { sender, meter_rx }
    }

    pub fn sender(&self) -> MixerCommandSender<MixerCommand> {
        self.sender.clone()
    }

    pub fn poll(&self, state: &mut MixerState) -> bool {
        let mut updated = false;
        self.meter_rx.drain(|event| {
            state.update_meter(
                event.ch,
                event.peak_l,
                event.peak_r,
                event.rms_l,
                event.rms_r,
                event.clip_l,
                event.clip_r,
            );
            updated = true;
        });
        updated
    }
}

pub struct MixerView {
    api: Arc<dyn MixerUiApi>,
    state: MixerState,
    master_cpu: f32,
    master_meter_db: (f32, f32),
    cpu_history: VecDeque<f32>,
    meter_history: VecDeque<(f32, f32)>,
    history_capacity: usize,
    last_history_update: Instant,
    skin: MixerSkin,
    graphs_visible: bool,
    #[cfg(feature = "mixer_api")]
    engine: Option<MixerEngineBridge>,
}

impl MixerView {
    #[cfg(feature = "mixer_api")]
    pub fn new(api: Arc<dyn MixerUiApi>, engine: Option<MixerEngineBridge>) -> Self {
        Self {
            api,
            state: MixerState::default(),
            master_cpu: 0.0,
            master_meter_db: (f32::NEG_INFINITY, f32::NEG_INFINITY),
            cpu_history: VecDeque::with_capacity(240),
            meter_history: VecDeque::with_capacity(240),
            history_capacity: 240,
            last_history_update: Instant::now(),
            skin: MixerSkin::Compact,
            graphs_visible: false,
            engine,
        }
    }

    #[cfg(not(feature = "mixer_api"))]
    pub fn new(api: Arc<dyn MixerUiApi>) -> Self {
        Self {
            api,
            state: MixerState::default(),
            master_cpu: 0.0,
            master_meter_db: (f32::NEG_INFINITY, f32::NEG_INFINITY),
            cpu_history: VecDeque::with_capacity(240),
            meter_history: VecDeque::with_capacity(240),
            history_capacity: 240,
            last_history_update: Instant::now(),
            skin: MixerSkin::Compact,
            graphs_visible: false,
        }
    }

    pub fn toggle_density(&mut self) {
        self.skin = self.skin.toggle();
    }

    pub fn zoom_in(&mut self) {}

    pub fn zoom_out(&mut self) {}

    pub fn ui(&mut self, ui: &mut Ui, palette: &HarmoniqPalette) {
        let snapshots = self.sync_from_api();
        let mut callbacks = self.build_callbacks(&snapshots);
        self.update_histories();
        self.render(ui, palette, &mut callbacks);
    }

    fn update_histories(&mut self) {
        let now = Instant::now();
        if now.saturating_duration_since(self.last_history_update) < Duration::from_millis(16) {
            return;
        }
        self.last_history_update = now;
        self.cpu_history.push_back(self.master_cpu);
        self.meter_history.push_back(self.master_meter_db);
        while self.cpu_history.len() > self.history_capacity {
            self.cpu_history.pop_front();
        }
        while self.meter_history.len() > self.history_capacity {
            self.meter_history.pop_front();
        }
    }

    fn render(&mut self, ui: &mut Ui, palette: &HarmoniqPalette, callbacks: &mut MixerCallbacks) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);

            self.render_header(ui, palette);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Routing Matrix").clicked() {
                    self.state.routing_visible = !self.state.routing_visible;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("Shift+double-click any meter to clear peaks")
                            .small()
                            .color(palette.text_muted),
                    );
                });
            });
            ui.add_space(4.0);
            self.render_strips(ui, palette, callbacks);
        });

        if self.state.routing_visible {
            self.render_routing_matrix(ui, callbacks);
        }
    }

    fn render_header(&mut self, ui: &mut Ui, palette: &HarmoniqPalette) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 2.0);
            ui.label(RichText::new("Mixer").strong());
            if let Some(selected_id) = self.state.selected {
                if let Some(channel) = self.state.channels.iter().find(|ch| ch.id == selected_id) {
                    ui.label(
                        RichText::new(format!("Selected: {}", channel.name))
                            .strong()
                            .color(palette.text_primary),
                    );
                }
            }

            let button_label = if self.graphs_visible {
                "Hide Graphs"
            } else {
                "Show Graphs"
            };
            if ui.button(button_label).clicked() {
                self.graphs_visible = !self.graphs_visible;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.render_skin_controls(ui);
                self.render_master_meter_summary(ui, palette);
                self.render_cpu_summary(ui);
            });
        });
        if self.graphs_visible {
            ui.add_space(6.0);
            self.render_rt_graphs(ui, palette);
        }
    }

    fn render_skin_controls(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new("Skin").small());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                for variant in MixerSkin::ALL {
                    let selected = self.skin == variant;
                    let response = ui.selectable_label(selected, variant.label());
                    if response.clicked() {
                        self.skin = variant;
                    }
                }
            });
        });
    }

    fn render_cpu_summary(&self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new("Engine Load").small());
            let pct = self.master_cpu.clamp(0.0, 100.0);
            let bar = egui::ProgressBar::new((pct / 100.0).clamp(0.0, 1.0))
                .desired_width(140.0)
                .desired_height(12.0)
                .text(format!("{pct:.1}%"));
            ui.add(bar);
        });
    }

    fn render_master_meter_summary(&self, ui: &mut Ui, palette: &HarmoniqPalette) {
        let (l_db, r_db) = self.master_meter_db;
        let left = if l_db.is_finite() { l_db } else { -120.0 };
        let right = if r_db.is_finite() { r_db } else { -120.0 };
        let clip = left >= 0.0 || right >= 0.0;
        let clip_color = if clip {
            palette.warning
        } else {
            palette.meter_background
        };

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new("Master Peak").small());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(format!("L {left:>6.1} dB"));
                ui.label(format!("R {right:>6.1} dB"));
                ui.colored_label(clip_color, RichText::new("●"));
            });
        });
    }

    fn render_rt_graphs(&self, ui: &mut Ui, palette: &HarmoniqPalette) {
        Frame::none()
            .fill(palette.panel_alt)
            .stroke(Stroke::new(1.0, palette.mixer_strip_border))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_height(110.0);
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 4.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new("CPU History").small());
                        self.render_cpu_history_plot(ui);
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new("Master Meter (dB)").small());
                        self.render_meter_history_plot(ui);
                    });
                });
            });
    }

    fn render_cpu_history_plot(&self, ui: &mut Ui) {
        let points = if self.cpu_history.is_empty() {
            PlotPoints::from_iter([[0.0, 0.0]].into_iter())
        } else {
            PlotPoints::from_iter(
                self.cpu_history
                    .iter()
                    .enumerate()
                    .map(|(idx, value)| [idx as f64, value.clamp(0.0, 100.0) as f64]),
            )
        };

        Plot::new("mixer_cpu_history")
            .view_aspect(1.8)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .include_y(0.0)
            .include_y(100.0)
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new(points)
                        .color(Color32::from_rgb(166, 104, 239))
                        .name("CPU %"),
                );
            });
    }

    fn render_meter_history_plot(&self, ui: &mut Ui) {
        let sanitize = |db: f32| -> f64 {
            if db.is_finite() {
                db.max(-120.0) as f64
            } else {
                -120.0
            }
        };

        let left_points = if self.meter_history.is_empty() {
            PlotPoints::from_iter([[0.0, -120.0]].into_iter())
        } else {
            PlotPoints::from_iter(
                self.meter_history
                    .iter()
                    .enumerate()
                    .map(|(idx, (left, _))| [idx as f64, sanitize(*left)]),
            )
        };

        let right_points = if self.meter_history.is_empty() {
            PlotPoints::from_iter([[0.0, -120.0]].into_iter())
        } else {
            PlotPoints::from_iter(
                self.meter_history
                    .iter()
                    .enumerate()
                    .map(|(idx, (_, right))| [idx as f64, sanitize(*right)]),
            )
        };

        Plot::new("mixer_meter_history")
            .view_aspect(1.8)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .include_y(-120.0)
            .include_y(6.0)
            .legend(Legend::default())
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new(left_points)
                        .color(Color32::from_rgb(94, 210, 170))
                        .name("Left"),
                );
                plot_ui.line(
                    Line::new(right_points)
                        .color(Color32::from_rgb(255, 150, 132))
                        .name("Right"),
                );
            });
    }

    fn render_strips(
        &mut self,
        ui: &mut Ui,
        palette: &HarmoniqPalette,
        callbacks: &mut MixerCallbacks,
    ) {
        let mut interactions = Vec::new();
        let channel_len = self.state.channels.len();
        let classic_slots = SlotSkin::classic(palette);
        let compact_style = CompactStripStyle::new(palette);
        ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.horizontal_top(|ui| {
                    for idx in 0..channel_len {
                        if let Some(ch) = self.state.channels.get_mut(idx) {
                            let is_selected = self.state.selected == Some(ch.id);
                            let interaction = match self.skin {
                                MixerSkin::Classic => Self::render_classic_channel_strip(
                                    ui,
                                    palette,
                                    ch,
                                    callbacks,
                                    is_selected,
                                    classic_slots,
                                ),
                                MixerSkin::Compact => Self::render_compact_channel_strip(
                                    ui,
                                    palette,
                                    ch,
                                    callbacks,
                                    is_selected,
                                    &compact_style,
                                ),
                            };
                            interactions.push(interaction);
                        }
                    }
                });
            });
        self.apply_strip_interactions(interactions);
    }

    fn apply_strip_interactions(&mut self, interactions: Vec<StripInteraction>) {
        let mut reset_all = false;
        let mut channel_resets = Vec::new();
        let mut selection_update: Option<Option<ChannelId>> = None;

        for interaction in interactions {
            if let Some(reset) = interaction.reset {
                match reset {
                    ResetRequest::All => {
                        reset_all = true;
                    }
                    ResetRequest::Channel(id) => channel_resets.push(id),
                }
            }

            if let Some(selection) = interaction.selection {
                selection_update = Some(match selection {
                    SelectionRequest::Select(id) => Some(id),
                    SelectionRequest::Clear => None,
                });
            }
        }

        if reset_all {
            self.state.reset_peaks_all();
        } else {
            for id in channel_resets {
                self.state.reset_peaks_for(id);
            }
        }

        if let Some(target) = selection_update {
            self.state.selected = target;
        }
    }

    fn render_classic_channel_strip(
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        ch: &mut Channel,
        callbacks: &mut MixerCallbacks,
        is_selected: bool,
        slot_skin: SlotSkin,
    ) -> StripInteraction {
        let mut interaction = StripInteraction::default();
        let fill = if is_selected {
            palette.mixer_strip_selected
        } else if ch.solo {
            palette.mixer_strip_solo
        } else if ch.mute {
            palette.mixer_strip_muted
        } else {
            palette.mixer_strip_bg
        };

        let frame = Frame::none()
            .fill(fill)
            .stroke(Stroke::new(1.0, palette.mixer_strip_border))
            .rounding(Rounding::same(12.0))
            .inner_margin(Margin::symmetric(12.0, 10.0));

        let response = frame.show(ui, |ui| {
            ui.set_width(200.0);
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);

            ui.horizontal(|ui| {
                let clip = ch.meter.clip_l || ch.meter.clip_r;
                let clip_color = if clip {
                    palette.warning
                } else {
                    palette.meter_background
                };
                ui.colored_label(clip_color, RichText::new("●"));
                ui.add(
                    egui::TextEdit::singleline(&mut ch.name)
                        .desired_width(140.0)
                        .font(egui::TextStyle::Button),
                );
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
                let avg_rms = ((ch.meter.rms_l + ch.meter.rms_r) * 0.5).clamp(0.0, 1.2);
                let meter_resp = ui
                    .add(
                        LevelMeter::new(palette)
                            .with_size(egui::vec2(24.0, 190.0))
                            .with_levels(
                                ch.meter.peak_l.clamp(0.0, 1.2).min(1.0),
                                ch.meter.peak_r.clamp(0.0, 1.2).min(1.0),
                                avg_rms.min(1.0),
                            ),
                    )
                    .on_hover_text("Double-click to reset peak. Hold Shift to reset all strips");
                Self::draw_clip_light(
                    ui,
                    meter_resp.rect,
                    ch.meter.clip_l || ch.meter.clip_r,
                    palette,
                );
                if meter_resp.double_clicked() {
                    if ui.input(|i| i.modifiers.shift) {
                        interaction.reset = Some(ResetRequest::All);
                    } else {
                        interaction.reset = Some(ResetRequest::Channel(ch.id));
                    }
                }

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 6.0);
                    let fader_resp = ui.add(
                        Fader::new(&mut ch.gain_db, -60.0, 12.0, 0.0, palette).with_height(190.0),
                    );
                    if fader_resp.changed() {
                        (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                    }
                    ui.label(RichText::new(format!("{:.1} dB", ch.gain_db)).small());
                });

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    let pan_resp = ui.add(
                        Knob::new(&mut ch.pan, -1.0, 1.0, 0.0, "Pan", palette).with_diameter(52.0),
                    );
                    if pan_resp.changed() {
                        (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                    }
                    ui.label(RichText::new(format!("{:.2}", ch.pan)).small());

                    ui.horizontal(|ui| {
                        let mute_resp = ui
                            .add(
                                StateToggleButton::new(&mut ch.mute, "M", palette).with_width(38.0),
                            )
                            .on_hover_text("Mute");
                        if mute_resp.changed() {
                            (callbacks.set_mute)(ch.id, ch.mute);
                        }
                        let solo_resp = ui
                            .add(
                                StateToggleButton::new(&mut ch.solo, "S", palette).with_width(38.0),
                            )
                            .on_hover_text("Solo");
                        if solo_resp.changed() {
                            (callbacks.set_solo)(ch.id, ch.solo);
                        }
                    });
                });
            });

            ui.separator();
            Self::render_inserts(ui, palette, ch, callbacks, slot_skin);

            if !ch.is_master {
                ui.separator();
                Self::render_sends(ui, palette, ch, callbacks, slot_skin, 40.0);
            }

            ui.add_space(6.0);
            let select_label = if is_selected { "Selected" } else { "Select" };
            let select_resp = ui.add(egui::SelectableLabel::new(is_selected, select_label));
            if select_resp.clicked() {
                interaction.selection = Some(if is_selected {
                    SelectionRequest::Clear
                } else {
                    SelectionRequest::Select(ch.id)
                });
            }
        });

        response.response.context_menu(|ui| {
            if ui.button("Add Insert…").clicked() {
                (callbacks.open_insert_browser)(ch.id, None);
                ui.close_menu();
            }
        });

        interaction
    }

    fn render_compact_channel_strip(
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        ch: &mut Channel,
        callbacks: &mut MixerCallbacks,
        is_selected: bool,
        style: &CompactStripStyle,
    ) -> StripInteraction {
        let mut interaction = StripInteraction::default();
        let (fill, stroke) = style.fill_for(ch, is_selected);
        let mut reset_request: Option<ResetRequest> = None;
        let mut selection_request: Option<SelectionRequest> = None;

        let frame = Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(Rounding::same(style.rounding))
            .inner_margin(style.inner_margin);

        let response = frame.show(ui, |ui| {
            ui.set_width(style.strip_width);
            ui.spacing_mut().item_spacing = style.item_spacing;

            Self::render_compact_header(ui, ch, style);

            ui.add_space(style.section_spacing);

            Self::render_compact_history(ui, ch, style);

            ui.add_space(style.section_spacing);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                let meter_resp = Self::render_compact_meter(ui, ch, style);
                if meter_resp.double_clicked() {
                    let all = ui.input(|i| i.modifiers.shift);
                    reset_request = Some(if all {
                        ResetRequest::All
                    } else {
                        ResetRequest::Channel(ch.id)
                    });
                }

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 6.0);
                    let fader_resp = ui.add(
                        Fader::new(&mut ch.gain_db, -60.0, 12.0, 0.0, palette)
                            .with_height(style.fader_height)
                            .with_width(style.fader_width),
                    );
                    if fader_resp.changed() {
                        (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                    }
                    ui.label(
                        RichText::new(format!("{:+.1} dB", ch.gain_db))
                            .small()
                            .color(style.label_primary),
                    );
                });

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    let pan_resp = ui.add(
                        Knob::new(&mut ch.pan, -1.0, 1.0, 0.0, "Pan", palette)
                            .with_diameter(style.knob_diameter),
                    );
                    if pan_resp.changed() {
                        (callbacks.set_gain_pan)(ch.id, ch.gain_db, ch.pan);
                    }
                    ui.label(
                        RichText::new(format!("{:+.2}", ch.pan))
                            .small()
                            .color(style.label_secondary),
                    );

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                        let mute_resp = Self::compact_toggle(ui, &mut ch.mute, "M", style)
                            .on_hover_text("Mute");
                        if mute_resp.changed() {
                            (callbacks.set_mute)(ch.id, ch.mute);
                        }
                        let solo_resp = Self::compact_toggle(ui, &mut ch.solo, "S", style)
                            .on_hover_text("Solo");
                        if solo_resp.changed() {
                            (callbacks.set_solo)(ch.id, ch.solo);
                        }
                    });
                });
            });

            ui.add_space(style.section_spacing);
            ui.separator();

            Self::render_inserts(ui, palette, ch, callbacks, style.slot_colors);

            if !ch.is_master {
                ui.separator();
                Self::render_sends(
                    ui,
                    palette,
                    ch,
                    callbacks,
                    style.slot_colors,
                    style.send_knob_diameter,
                );
            }

            ui.add_space(style.section_spacing * 0.6);
            let select_label = if is_selected { "Selected" } else { "Select" };
            let select_resp = ui.add(egui::SelectableLabel::new(is_selected, select_label));
            if select_resp.clicked() {
                selection_request = Some(if is_selected {
                    SelectionRequest::Clear
                } else {
                    SelectionRequest::Select(ch.id)
                });
            }
        });

        if let Some(request) = reset_request {
            interaction.reset = Some(request);
        }
        if let Some(selection) = selection_request {
            interaction.selection = Some(selection);
        }

        response.response.context_menu(|ui| {
            if ui.button("Add Insert…").clicked() {
                (callbacks.open_insert_browser)(ch.id, None);
                ui.close_menu();
            }
        });

        interaction
    }

    fn render_compact_header(ui: &mut egui::Ui, ch: &mut Channel, style: &CompactStripStyle) {
        let rounding = Rounding {
            nw: style.rounding,
            ne: style.rounding,
            sw: 4.0,
            se: 4.0,
        };
        Frame::none()
            .fill(style.header_fill)
            .stroke(Stroke::new(1.0, style.meter_border))
            .rounding(rounding)
            .inner_margin(Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.scope(|ui| {
                    ui.visuals_mut().override_text_color = Some(style.header_text);
                    ui.add(
                        egui::TextEdit::singleline(&mut ch.name)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Small),
                    );
                });
            });
    }

    fn render_compact_history(ui: &mut egui::Ui, ch: &Channel, style: &CompactStripStyle) {
        let width = ui.available_width();
        let width = if width.is_finite() {
            width
        } else {
            style.strip_width
        };
        let size = egui::vec2(width, style.history_height);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, style.history_rounding, style.history_fill);
        painter.rect_stroke(
            rect,
            style.history_rounding,
            Stroke::new(1.0, style.meter_border),
        );

        for db in [-24.0_f32, -12.0, -6.0, 0.0] {
            let level = db_to_gain(db).clamp(0.0, 1.0);
            let y = rect.bottom() - rect.height() * level;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(1.0, style.history_grid),
            );
        }

        let values: Vec<f32> = ch.meter_history.iter().copied().collect();
        if values.len() > 1 {
            let step = if values.len() > 1 {
                rect.width() / (values.len() as f32 - 1.0)
            } else {
                rect.width()
            };

            let mut points = Vec::with_capacity(values.len());
            for (idx, value) in values.iter().enumerate() {
                let x = rect.left() + step * idx as f32;
                let y = rect.bottom() - rect.height() * value.clamp(0.0, 1.0);
                points.push(egui::pos2(x, y));
            }

            if points.len() >= 3 {
                let mut area = points.clone();
                area.push(egui::pos2(rect.right(), rect.bottom()));
                area.push(egui::pos2(rect.left(), rect.bottom()));
                painter.add(egui::Shape::convex_polygon(
                    area,
                    style.history_glow,
                    Stroke::NONE,
                ));
            }

            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(1.5, style.history_line),
            ));

            if let Some(last) = points.last() {
                painter.circle_filled(*last, 3.0, style.history_line);
            }
        }

        painter.text(
            rect.left_top() + egui::vec2(8.0, 6.0),
            Align2::LEFT_TOP,
            "RMS",
            FontId::proportional(10.0),
            style.label_secondary,
        );
    }

    fn render_compact_meter(
        ui: &mut egui::Ui,
        ch: &Channel,
        style: &CompactStripStyle,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(style.meter_size, egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, style.meter_rounding, style.meter_bg);
        painter.rect_stroke(
            rect,
            style.meter_rounding,
            Stroke::new(1.0, style.meter_border),
        );

        let meter_rect = rect.shrink2(egui::vec2(4.0, 8.0));
        let peak = ch
            .meter
            .peak_l
            .max(ch.meter.peak_r)
            .clamp(0.0, 1.2)
            .min(1.0);
        let rms = ((ch.meter.rms_l + ch.meter.rms_r) * 0.5)
            .clamp(0.0, 1.2)
            .min(1.0);
        let hold = ch
            .meter
            .peak_hold_l
            .max(ch.meter.peak_hold_r)
            .clamp(0.0, 1.2)
            .min(1.0);

        let peak_height = meter_rect.height() * peak;
        let peak_rect = egui::Rect::from_min_max(
            egui::pos2(meter_rect.left(), meter_rect.bottom() - peak_height),
            egui::pos2(meter_rect.right(), meter_rect.bottom()),
        );
        painter.rect_filled(peak_rect, style.meter_rounding, style.meter_peak);

        let rms_height = meter_rect.height() * rms;
        let rms_width = meter_rect.width() * 0.55;
        let rms_rect = egui::Rect::from_min_max(
            egui::pos2(
                meter_rect.center().x - rms_width * 0.5,
                meter_rect.bottom() - rms_height,
            ),
            egui::pos2(meter_rect.center().x + rms_width * 0.5, meter_rect.bottom()),
        );
        painter.rect_filled(rms_rect, style.meter_rounding, style.meter_rms);

        let hold_y = meter_rect.bottom() - meter_rect.height() * hold;
        painter.line_segment(
            [
                egui::pos2(meter_rect.left(), hold_y),
                egui::pos2(meter_rect.right(), hold_y),
            ],
            Stroke::new(1.4, style.meter_hold),
        );

        for db in [-24.0_f32, -12.0, -6.0, 0.0] {
            let level = db_to_gain(db).clamp(0.0, 1.0);
            let y = meter_rect.bottom() - level * meter_rect.height();
            painter.line_segment(
                [
                    egui::pos2(meter_rect.left() - 4.0, y),
                    egui::pos2(meter_rect.left(), y),
                ],
                Stroke::new(1.0, style.meter_tick),
            );
        }

        let led_center = egui::pos2(rect.center().x, rect.top() + 6.0);
        let led_color = if ch.meter.clip_l || ch.meter.clip_r {
            style.clip_on
        } else {
            style.clip_off
        };
        painter.circle_filled(led_center, 4.0, led_color);

        response
    }

    fn compact_toggle(
        ui: &mut egui::Ui,
        value: &mut bool,
        label: &str,
        style: &CompactStripStyle,
    ) -> egui::Response {
        let (rect, mut response) =
            ui.allocate_exact_size(egui::vec2(style.toggle_width, 24.0), egui::Sense::click());
        if response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        let fill = if *value {
            style.meter_peak
        } else {
            style.meter_bg.gamma_multiply(1.35)
        };
        let text_color = if *value {
            style.header_text
        } else {
            style.label_secondary
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 5.0, fill);
        painter.rect_stroke(rect, 5.0, style.border);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.0),
            text_color,
        );
        response
    }

    fn render_inserts(
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        ch: &mut Channel,
        callbacks: &mut MixerCallbacks,
        slot_skin: SlotSkin,
    ) {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        ui.label(RichText::new("INSERTS").small().color(palette.text_muted));

        let drag_id = egui::Id::new(("mixer_insert_drag", ch.id));
        let pointer_pos = ui.ctx().pointer_interact_pos();
        let mut drop_target: Option<usize> = None;
        let mut pending_move: Option<(usize, usize)> = None;

        for idx in 0..ch.inserts.len() {
            let mut drop_request = None;
            let label = if ch.inserts[idx].name.is_empty() {
                "Empty".to_string()
            } else {
                ch.inserts[idx].name.clone()
            };

            let inner = Frame::none()
                .fill(slot_skin.empty_fill)
                .stroke(Stroke::new(1.0, slot_skin.border))
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let handle = ui
                            .add(egui::Label::new("≡").sense(egui::Sense::drag()))
                            .on_hover_text("Drag to reorder");
                        if handle.drag_started() {
                            ui.ctx().data_mut(|data| data.insert_temp(drag_id, idx));
                        }
                        if handle.drag_released() {
                            if let Some(from) =
                                ui.ctx().data(|data| data.get_temp::<usize>(drag_id))
                            {
                                drop_request = Some((from, drop_target.unwrap_or(idx)));
                            }
                            ui.ctx().data_mut(|data| data.remove::<usize>(drag_id));
                        }

                        let bypass_resp = ui
                            .add(
                                StateToggleButton::new(&mut ch.inserts[idx].bypass, "BYP", palette)
                                    .with_width(slot_skin.bypass_width),
                            )
                            .on_hover_text("Toggle bypass");
                        if bypass_resp.changed() {
                            (callbacks.set_insert_bypass)(ch.id, idx, ch.inserts[idx].bypass);
                        }

                        if ui
                            .add(
                                egui::Button::new(RichText::new(label.clone()).small())
                                    .fill(slot_skin.active_fill),
                            )
                            .clicked()
                        {
                            (callbacks.open_insert_ui)(ch.id, idx);
                        }

                        if ui
                            .add(egui::Button::new("✕").min_size(egui::vec2(24.0, 24.0)))
                            .on_hover_text("Remove")
                            .clicked()
                        {
                            (callbacks.remove_insert)(ch.id, idx);
                        }
                    });
                });

            let row_rect = inner.response.rect;
            if let Some((from, target)) = drop_request {
                pending_move = Some((from, target));
            }

            if let Some(pos) = pointer_pos {
                if row_rect.contains(pos) {
                    drop_target = Some(idx);
                    let stroke = Stroke::new(1.0, slot_skin.highlight);
                    ui.painter().rect_stroke(row_rect.expand(2.0), 6.0, stroke);
                }
            }
        }

        if ui
            .ctx()
            .data(|data| data.get_temp::<usize>(drag_id))
            .is_some()
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
            .add(
                egui::Button::new(RichText::new("+ Add Insert").small()).fill(slot_skin.empty_fill),
            )
            .clicked()
        {
            (callbacks.open_insert_browser)(ch.id, None);
        }
    }

    fn render_sends(
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        ch: &mut Channel,
        callbacks: &mut MixerCallbacks,
        slot_skin: SlotSkin,
        knob_diameter: f32,
    ) {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        ui.label(RichText::new("SENDS").small().color(palette.text_muted));

        for send in &mut ch.sends {
            Frame::none()
                .fill(slot_skin.empty_fill)
                .stroke(Stroke::new(1.0, slot_skin.border))
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let label = char::from(b'A'.saturating_add(send.id.min(25))).to_string();
                        ui.label(RichText::new(label).strong());
                        let knob_resp = ui.add(
                            Knob::new(&mut send.level, 0.0, 1.0, 0.0, "", palette)
                                .with_diameter(knob_diameter),
                        );
                        if knob_resp.changed() {
                            (callbacks.configure_send)(ch.id, send.id, send.level);
                        }
                        ui.label(RichText::new(format!("{:.0}%", send.level * 100.0)).small());
                    });
                });
        }
    }

    fn draw_clip_light(ui: &mut egui::Ui, rect: egui::Rect, clip: bool, palette: &HarmoniqPalette) {
        let radius = 5.0;
        let center = egui::pos2(rect.center().x, rect.top() + radius + 4.0);
        let color = if clip {
            palette.warning
        } else {
            palette.meter_background.gamma_multiply(0.7)
        };
        ui.painter().circle_filled(center, radius, color);
    }

    fn render_routing_matrix(&mut self, ui: &mut Ui, callbacks: &mut MixerCallbacks) {
        let mut open = self.state.routing_visible;
        egui::Window::new("Routing Matrix")
            .open(&mut open)
            .collapsible(false)
            .default_size(egui::vec2(720.0, 420.0))
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Level 0..1. Click cell to toggle; drag to adjust.");
                    if ui.button("Close").clicked() {
                        self.state.routing_visible = false;
                    }
                });
                ui.separator();

                let mut buses: BTreeSet<String> = BTreeSet::new();
                buses.insert("MASTER".to_string());
                for map in self.state.routing.routes.values() {
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
                        for ch in self.state.channels.iter().filter(|c| !c.is_master) {
                            grid_ui.label(ch.name.clone());
                            for bus in &buses {
                                let current = self.state.routing.level(ch.id, bus).unwrap_or(0.0);
                                let cell_id = grid_ui.make_persistent_id(("route", ch.id, bus));
                                let (rect, _) = grid_ui.allocate_exact_size(
                                    egui::vec2(80.0, 22.0),
                                    egui::Sense::click_and_drag(),
                                );
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
                                        grid_ui.visuals().widgets.noninteractive.fg_stroke.color,
                                    ),
                                );
                                painter.text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    format!("{current:.2}"),
                                    egui::TextStyle::Small.resolve(grid_ui.style()),
                                    grid_ui.visuals().text_color(),
                                );

                                let response =
                                    grid_ui.interact(rect, cell_id, egui::Sense::click_and_drag());
                                let mut level = current;
                                if response.clicked() {
                                    if (level - 0.0).abs() < f32::EPSILON {
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
                            self.state.routing.apply_delta(&delta);
                            (callbacks.apply_routing)(delta);
                        }
                    });
            });
        self.state.routing_visible = open;
    }

    pub fn cpu_estimate(&self) -> f32 {
        self.master_cpu
    }

    pub fn master_meter(&self) -> (f32, f32) {
        self.master_meter_db
    }

    #[cfg(feature = "mixer_api")]
    pub fn poll_engine(&mut self) -> bool {
        if let Some(engine) = &self.engine {
            engine.poll(&mut self.state)
        } else {
            false
        }
    }

    #[cfg(not(feature = "mixer_api"))]
    pub fn poll_engine(&mut self) -> bool {
        false
    }

    fn sync_from_api(&mut self) -> HashMap<ChannelId, StripSnapshot> {
        let total = self.api.strips_len();
        let mut snapshots = HashMap::with_capacity(total);
        let previous_selection = self.state.selected;
        let mut previous_meters: HashMap<ChannelId, (Meter, VecDeque<f32>)> = self
            .state
            .channels
            .iter()
            .map(|channel| {
                (
                    channel.id,
                    (channel.meter.clone(), channel.meter_history.clone()),
                )
            })
            .collect();
        self.state.channels.clear();

        for idx in 0..total {
            let info = self.api.strip_info(idx);
            let snapshot = self.populate_channel(idx, &info, previous_meters.remove(&info.id));
            snapshots.insert(info.id, snapshot);
        }

        if let Some(selected) = previous_selection {
            if self.state.channels.iter().any(|ch| ch.id == selected) {
                self.state.selected = Some(selected);
            } else {
                self.state.selected = None;
            }
        } else {
            self.state.selected = None;
        }

        snapshots
    }

    fn populate_channel(
        &mut self,
        idx: usize,
        info: &UiStripInfo,
        previous_state: Option<(Meter, VecDeque<f32>)>,
    ) -> StripSnapshot {
        let mut channel = Channel {
            id: info.id,
            name: info.name.clone(),
            gain_db: info.fader_db,
            pan: info.pan,
            mute: info.muted,
            solo: info.soloed,
            inserts: Vec::with_capacity(info.insert_count),
            sends: Vec::with_capacity(info.send_count),
            meter: Meter::default(),
            meter_history: previous_state
                .as_ref()
                .map(|(_, history)| history.clone())
                .unwrap_or_else(Channel::new_meter_history),
            is_master: info.is_master,
        };

        let mut insert_bypass = Vec::with_capacity(info.insert_count);
        for slot in 0..info.insert_count {
            let bypass = self.api.insert_is_bypassed(idx, slot);
            let label = self.api.insert_label(idx, slot);
            channel.inserts.push(InsertSlot {
                name: label,
                bypass,
            });
            insert_bypass.push(bypass);
        }

        let mut send_levels_db = Vec::with_capacity(info.send_count);
        for slot in 0..info.send_count {
            let level_db = self.api.send_level(idx, slot);
            let level = db_to_gain(level_db).clamp(0.0, 2.0);
            channel.sends.push(SendSlot {
                id: slot as u8,
                level,
            });
            send_levels_db.push(level_db);
        }

        let mut meter = previous_state.map(|(meter, _)| meter).unwrap_or_default();
        #[cfg(feature = "mixer_api")]
        let use_engine_meters = self.engine.is_some();
        #[cfg(not(feature = "mixer_api"))]
        let use_engine_meters = false;
        let mut master_peak_db = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        if !use_engine_meters {
            let (peak_l_db, peak_r_db, rms_l_db, rms_r_db, clipped) = self.api.level_fetch(idx);
            meter.peak_l = db_to_linear(peak_l_db);
            meter.peak_r = db_to_linear(peak_r_db);
            meter.rms_l = db_to_linear(rms_l_db);
            meter.rms_r = db_to_linear(rms_r_db);
            meter.peak_hold_l = meter.peak_l;
            meter.peak_hold_r = meter.peak_r;
            meter.clip_l = clipped;
            meter.clip_r = clipped;
            meter.last_update = Instant::now();
            master_peak_db = (peak_l_db, peak_r_db);
        } else if info.is_master {
            master_peak_db = (
                gain_to_db(meter.peak_l).clamp(-120.0, 6.0),
                gain_to_db(meter.peak_r).clamp(-120.0, 6.0),
            );
        }
        channel.meter = meter;

        if info.is_master {
            self.master_cpu = info.cpu_percent;
            self.master_meter_db = master_peak_db;
        }

        self.state.channels.push(channel);

        StripSnapshot {
            index: idx,
            mute: info.muted,
            solo: info.soloed,
            insert_bypass,
            send_levels_db,
        }
    }

    fn build_callbacks(&self, snapshots: &HashMap<ChannelId, StripSnapshot>) -> MixerCallbacks {
        let mut callbacks = MixerCallbacks::noop();

        #[cfg(feature = "mixer_api")]
        let engine_sender = self.engine.as_ref().map(|bridge| bridge.sender());

        let api_gain = Arc::clone(&self.api);
        let map_gain = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_gain = engine_sender.clone();
        callbacks.set_gain_pan = Box::new(move |channel_id, db, pan| {
            if let Some(snapshot) = map_gain.get(&channel_id) {
                api_gain.set_fader_db(snapshot.index, db);
                api_gain.set_pan(snapshot.index, pan);
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_gain {
                let _ = tx.send(MixerCommand::SetGainPan {
                    ch: channel_id,
                    gain_db: db,
                    pan,
                });
            }
        });

        let api_mute = Arc::clone(&self.api);
        let map_mute = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_mute = engine_sender.clone();
        callbacks.set_mute = Box::new(move |channel_id, mute| {
            if let Some(snapshot) = map_mute.get(&channel_id) {
                if snapshot.mute != mute {
                    api_mute.toggle_mute(snapshot.index);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_mute {
                let _ = tx.send(MixerCommand::SetMute {
                    ch: channel_id,
                    mute,
                });
            }
        });

        let api_solo = Arc::clone(&self.api);
        let map_solo = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_solo = engine_sender.clone();
        callbacks.set_solo = Box::new(move |channel_id, solo| {
            if let Some(snapshot) = map_solo.get(&channel_id) {
                if snapshot.solo != solo {
                    api_solo.toggle_solo(snapshot.index);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_solo {
                let _ = tx.send(MixerCommand::SetSolo {
                    ch: channel_id,
                    solo,
                });
            }
        });

        let api_bypass = Arc::clone(&self.api);
        let map_bypass = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_bypass = engine_sender.clone();
        callbacks.set_insert_bypass = Box::new(move |channel_id, slot, bypass| {
            if let Some(snapshot) = map_bypass.get(&channel_id) {
                if snapshot.insert_bypass.get(slot).copied().unwrap_or(false) != bypass {
                    api_bypass.insert_toggle_bypass(snapshot.index, slot);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_bypass {
                let _ = tx.send(MixerCommand::SetInsertBypass {
                    ch: channel_id,
                    slot,
                    bypass,
                });
            }
        });

        let api_reorder = Arc::clone(&self.api);
        let map_reorder = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_reorder = engine_sender.clone();
        callbacks.reorder_insert = Box::new(move |channel_id, from, to| {
            if let Some(snapshot) = map_reorder.get(&channel_id) {
                api_reorder.insert_move(snapshot.index, from, to);
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_reorder {
                let _ = tx.send(MixerCommand::ReorderInsert {
                    ch: channel_id,
                    from,
                    to,
                });
            }
        });

        let api_send = Arc::clone(&self.api);
        let map_send = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_send = engine_sender.clone();
        callbacks.configure_send = Box::new(move |channel_id, send_id, level| {
            if let Some(snapshot) = map_send.get(&channel_id) {
                let target_db = gain_to_db(level).clamp(-60.0, 6.0);
                let previous = snapshot
                    .send_levels_db
                    .get(send_id as usize)
                    .copied()
                    .unwrap_or(f32::NEG_INFINITY);
                if (target_db - previous).abs() > 0.1 {
                    api_send.send_set_level(snapshot.index, send_id as usize, target_db);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_send {
                let _ = tx.send(MixerCommand::ConfigureSend {
                    ch: channel_id,
                    id: send_id,
                    level,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_insert_browser = engine_sender.clone();
        callbacks.open_insert_browser = Box::new(move |channel_id, slot| {
            info!(?channel_id, slot, "open_insert_browser");
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_insert_browser {
                let _ = tx.send(MixerCommand::OpenInsertBrowser {
                    ch: channel_id,
                    slot,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_insert_ui = engine_sender.clone();
        callbacks.open_insert_ui = Box::new(move |channel_id, slot| {
            info!(?channel_id, slot, "open_insert_ui");
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_insert_ui {
                let _ = tx.send(MixerCommand::OpenInsertUi {
                    ch: channel_id,
                    slot,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_remove_insert = engine_sender;
        callbacks.remove_insert = Box::new(move |channel_id, slot| {
            warn!(?channel_id, slot, "remove_insert_unimplemented");
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_remove_insert {
                let _ = tx.send(MixerCommand::RemoveInsert {
                    ch: channel_id,
                    slot,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_routing = engine_sender;
        callbacks.apply_routing = Box::new(move |delta| {
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_routing {
                let cmd = MixerCommand::ApplyRouting {
                    set: delta.set.clone(),
                    remove: delta.remove.clone(),
                };
                let _ = tx.send(cmd);
            }
        });

        callbacks
    }
}

fn db_to_linear(db: f32) -> f32 {
    if db.is_finite() {
        (10.0f32).powf(db * 0.05)
    } else {
        0.0
    }
}
