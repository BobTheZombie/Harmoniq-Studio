use crate::{
    state::{ChannelId, SendId},
    MixerCallbacks,
};
use egui::{
    self, Align, Align2, Color32, Frame, Layout, Margin, Pos2, Rect, Response, Rounding, Sense,
    Stroke, Ui, Vec2,
};

const DB_MIN: f32 = -60.0;
const DB_MAX: f32 = 12.0;
const MAX_RACK_SLOTS: usize = 10;

/// Insert slot entry metadata for the rack section.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    pub on: bool,
}

/// Send slot entry metadata for the rack section.
#[derive(Clone, Debug, PartialEq)]
pub struct SendSlot {
    pub id: SendId,
    pub dest: String,
    pub gain: f32,
    pub pre: bool,
    pub on: bool,
}

/// Mixer strip data model consumed by the UI skin.
#[derive(Clone, Debug, PartialEq)]
pub struct StripModel {
    pub channel_id: ChannelId,
    pub name: String,
    pub color: Color32,
    pub meter_l: f32,
    pub meter_r: f32,
    pub peak_l: f32,
    pub peak_r: f32,
    pub clip: bool,
    pub gain_db: f32,
    pub pan: f32,
    pub width: f32,
    pub mute: bool,
    pub solo: bool,
    pub rec: bool,
    pub inserts: Vec<Slot>,
    pub sends: Vec<SendSlot>,
}

/// Layout density presets for the mixer strips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Narrow,
    Wide,
}

/// Token bag describing the look & feel of the mixer skin.
#[derive(Clone, Debug)]
pub struct MixerTheme {
    pub colors: ThemeColors,
    pub sizes: ThemeSizes,
    zoom: f32,
    rounding: f32,
    pub chrome_stroke: Stroke,
}

#[derive(Clone, Debug)]
pub struct ThemeColors {
    pub background: Color32,
    pub panel: Color32,
    pub strip_bg: Color32,
    pub strip_bg_alt: Color32,
    pub master_strip_bg: Color32,
    pub meter_background: Color32,
    pub meter_low: Color32,
    pub meter_mid: Color32,
    pub meter_high: Color32,
    pub meter_peak: Color32,
    pub meter_clip: Color32,
    pub meter_grid: Color32,
    pub meter_border: Color32,
    pub text_primary: Color32,
    pub text_dim: Color32,
    pub button_on: Color32,
    pub button_off: Color32,
    pub button_mute_on: Color32,
    pub button_solo_on: Color32,
    pub button_rec_on: Color32,
    pub button_badge: Color32,
    pub rack_header: Color32,
    pub rack_panel: Color32,
    pub strip_header: Color32,
    pub fader_track: Color32,
    pub fader_track_inner: Color32,
    pub fader_handle: Color32,
}

#[derive(Clone, Debug)]
pub struct ThemeSizes {
    pub strip_width_narrow: f32,
    pub strip_width_wide: f32,
    pub fader_height: f32,
    pub fader_width: f32,
    pub meter_width: f32,
    pub knob_diameter: f32,
    pub spacing: f32,
    pub inner_spacing: f32,
    pub rack_row_height: f32,
    pub rack_section_title: f32,
    pub button_height: f32,
    pub strip_inner_margin: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ScaledSizes {
    pub zoom: f32,
    pub strip_width_narrow: f32,
    pub strip_width_wide: f32,
    pub fader_height: f32,
    pub fader_width: f32,
    pub meter_width: f32,
    pub knob_diameter: f32,
    pub spacing: f32,
    pub inner_spacing: f32,
    pub rack_row_height: f32,
    pub rack_section_title: f32,
    pub button_height: f32,
    pub strip_inner_margin: f32,
}

impl Default for MixerTheme {
    fn default() -> Self {
        Self {
            colors: ThemeColors {
                background: Color32::from_rgb(0x16, 0x18, 0x1F),
                panel: Color32::from_rgb(0x1C, 0x20, 0x28),
                strip_bg: Color32::from_rgb(0x21, 0x26, 0x31),
                strip_bg_alt: Color32::from_rgb(0x1D, 0x22, 0x2B),
                master_strip_bg: Color32::from_rgb(0x27, 0x2E, 0x3C),
                meter_background: Color32::from_rgb(0x0B, 0x0E, 0x14),
                meter_low: Color32::from_rgb(0x3C, 0xC6, 0x9A),
                meter_mid: Color32::from_rgb(0xF1, 0xC4, 0x61),
                meter_high: Color32::from_rgb(0xF0, 0x90, 0x5C),
                meter_peak: Color32::from_rgb(0xF2, 0x60, 0x66),
                meter_clip: Color32::from_rgb(0xFF, 0x4B, 0x4B),
                meter_grid: Color32::from_rgb(0x29, 0x2F, 0x3A),
                meter_border: Color32::from_rgb(0x37, 0x3E, 0x4B),
                text_primary: Color32::from_rgb(0xF0, 0xF3, 0xF8),
                text_dim: Color32::from_rgb(0x9D, 0xA7, 0xB8),
                button_on: Color32::from_rgb(0x28, 0x36, 0x45),
                button_off: Color32::from_rgb(0x19, 0x1F, 0x27),
                button_mute_on: Color32::from_rgb(0x36, 0x9B, 0xE6),
                button_solo_on: Color32::from_rgb(0xF7, 0xC0, 0x4C),
                button_rec_on: Color32::from_rgb(0xF2, 0x5B, 0x5B),
                button_badge: Color32::from_rgb(0x12, 0x18, 0x22),
                rack_header: Color32::from_rgb(0xD2, 0xD8, 0xE4),
                rack_panel: Color32::from_rgb(0x1A, 0x1F, 0x28),
                strip_header: Color32::from_rgb(0x18, 0x1E, 0x27),
                fader_track: Color32::from_rgb(0x12, 0x17, 0x21),
                fader_track_inner: Color32::from_rgb(0x1C, 0x23, 0x30),
                fader_handle: Color32::from_rgb(0xC8, 0xD2, 0xDE),
            },
            sizes: ThemeSizes {
                strip_width_narrow: 68.0,
                strip_width_wide: 112.0,
                fader_height: 260.0,
                fader_width: 26.0,
                meter_width: 16.0,
                knob_diameter: 28.0,
                spacing: 6.0,
                inner_spacing: 8.0,
                rack_row_height: 18.0,
                rack_section_title: 13.0,
                button_height: 18.0,
                strip_inner_margin: 7.0,
            },
            zoom: 1.0,
            rounding: 3.0,
            chrome_stroke: Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 190)),
        }
    }
}

impl MixerTheme {
    /// Update the cached zoom for subsequent scaling operations.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.5, 2.0);
    }

    /// Current zoom factor.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Produce zoom-adjusted metrics for layout.
    pub fn scaled_sizes(&self) -> ScaledSizes {
        ScaledSizes {
            zoom: self.zoom,
            strip_width_narrow: self.sizes.strip_width_narrow * self.zoom,
            strip_width_wide: self.sizes.strip_width_wide * self.zoom,
            fader_height: self.sizes.fader_height * self.zoom,
            fader_width: self.sizes.fader_width * self.zoom,
            meter_width: self.sizes.meter_width * self.zoom,
            knob_diameter: self.sizes.knob_diameter * self.zoom,
            spacing: self.sizes.spacing * self.zoom,
            inner_spacing: self.sizes.inner_spacing * self.zoom,
            rack_row_height: self.sizes.rack_row_height * self.zoom,
            rack_section_title: self.sizes.rack_section_title * self.zoom,
            button_height: self.sizes.button_height * self.zoom,
            strip_inner_margin: self.sizes.strip_inner_margin * self.zoom,
        }
    }

    /// Rounded corner radius based on zoom.
    pub fn rounding(&self) -> f32 {
        self.rounding * self.zoom
    }

    /// Paint meter bar for a single channel inside the given rect.
    pub fn paint_meter(&self, painter: &egui::Painter, rect: Rect, value: f32, left: bool) {
        let level = value.clamp(0.0, 1.0);
        let lane = self.meter_lane(rect, left);
        let height = lane.height();
        let bottom = lane.bottom();
        let segments = [
            (0.0, 0.6, self.colors.meter_low),
            (0.6, 0.82, self.colors.meter_mid),
            (0.82, 0.96, self.colors.meter_high),
            (0.96, 1.0, self.colors.meter_peak),
        ];

        for &(start, end, color) in &segments {
            if level <= start {
                continue;
            }
            let seg_end = level.min(end);
            if seg_end <= start {
                continue;
            }

            let start_y = bottom - start * height;
            let end_y = bottom - seg_end * height;
            if end_y >= start_y {
                continue;
            }

            let segment_rect = Rect::from_min_max(
                Pos2::new(lane.left(), end_y),
                Pos2::new(lane.right(), start_y),
            );
            let rounding = if seg_end >= 0.99 {
                Rounding::same(self.rounding() * 0.25)
            } else {
                Rounding::same(self.rounding() * 0.05)
            };
            painter.rect_filled(segment_rect, rounding, color);
        }
    }

    /// Paint peak hold line overlay for a single meter.
    pub fn paint_peak_line(&self, painter: &egui::Painter, rect: Rect, peak: f32, left: bool) {
        let clamped = peak.clamp(0.0, 1.0);
        let lane = self.meter_lane(rect, left);
        let y = lane.bottom() - clamped * lane.height();
        painter.line_segment(
            [Pos2::new(lane.left(), y), Pos2::new(lane.right(), y)],
            Stroke::new(1.0, self.colors.text_dim),
        );
    }

    /// Paint a clip LED at the top of the meter pair.
    pub fn paint_clip_led(&self, painter: &egui::Painter, rect: Rect, clip: bool) {
        let led_height = (rect.height() * 0.08).clamp(6.0, 10.0);
        for left in [true, false] {
            let lane = self.meter_lane(rect, left);
            let led_rect = Rect::from_center_size(
                Pos2::new(lane.center().x, rect.top() + led_height * 0.6),
                Vec2::new(lane.width() * 0.9, led_height),
            );
            let color = if clip {
                self.colors.meter_clip
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 150)
            };
            painter.rect_filled(led_rect, Rounding::same(led_height * 0.35), color);
            painter.rect_stroke(
                led_rect,
                Rounding::same(led_height * 0.35),
                Stroke::new(1.0, self.colors.meter_border),
            );
        }
    }

    fn meter_lane(&self, rect: Rect, left: bool) -> Rect {
        let gutter = 3.0 * self.zoom();
        let lane_width = rect.width() * 0.5 - gutter * 1.5;
        let lane_left = if left {
            rect.left() + gutter
        } else {
            rect.center().x + gutter * 0.5
        };
        Rect::from_min_max(
            Pos2::new(lane_left, rect.top() + gutter),
            Pos2::new(lane_left + lane_width, rect.bottom() - gutter),
        )
    }
}

/// Mixer UI controller that renders strips based on [`StripModel`] inputs.
pub struct MixerUi {
    pub strips: Vec<StripModel>,
    pub master: StripModel,
    pub density: Density,
    pub zoom: f32,
    pub show_meter_bridge: bool,
    pub theme: MixerTheme,
}

impl MixerUi {
    /// Render the mixer UI using egui primitives.
    pub fn ui(&mut self, ui: &mut egui::Ui, callbacks: &mut MixerCallbacks) {
        self.zoom = self.zoom.clamp(0.85, 1.35);
        self.theme.set_zoom(self.zoom);

        let mut zoom_delta = 0.0f32;
        let mut pending_density = None;
        ui.ctx().input(|input| {
            if input.key_pressed(egui::Key::N) {
                pending_density = Some(Density::Narrow);
            }
            if input.key_pressed(egui::Key::W) {
                pending_density = Some(Density::Wide);
            }
            if input.modifiers.command_only() {
                if input.key_pressed(egui::Key::Equals) {
                    zoom_delta += 0.05;
                }
                if input.key_pressed(egui::Key::Minus) {
                    zoom_delta -= 0.05;
                }
            }
        });
        if let Some(d) = pending_density {
            self.density = d;
        }
        if zoom_delta.abs() > f32::EPSILON {
            self.zoom = (self.zoom + zoom_delta).clamp(0.85, 1.35);
            self.theme.set_zoom(self.zoom);
        }

        let sizes = self.theme.scaled_sizes();

        Frame::none()
            .fill(self.theme.colors.background)
            .rounding(Rounding::same(self.theme.rounding()))
            .inner_margin(Margin::symmetric(sizes.spacing * 0.8, sizes.spacing * 0.8))
            .show(ui, |ui| {
                Frame::none()
                    .fill(self.theme.colors.panel)
                    .rounding(Rounding::same(self.theme.rounding() * 0.85))
                    .inner_margin(Margin::symmetric(sizes.spacing, sizes.spacing))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(sizes.spacing);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(sizes.spacing);
                            ui.toggle_value(&mut self.show_meter_bridge, "Meter Bridge")
                                .on_hover_text("Toggle the top meter bridge");

                            let narrow = density_button(
                                ui,
                                "Narrow",
                                matches!(self.density, Density::Narrow),
                                &self.theme,
                            );
                            if narrow.clicked() {
                                self.density = Density::Narrow;
                            }
                            let wide = density_button(
                                ui,
                                "Wide",
                                matches!(self.density, Density::Wide),
                                &self.theme,
                            );
                            if wide.clicked() {
                                self.density = Density::Wide;
                            }

                            ui.separator();

                            let mut slider = egui::Slider::new(&mut self.zoom, 0.85..=1.35)
                                .text("Zoom")
                                .step_by(0.01);
                            slider = slider.clamp_to_range(true);
                            if ui
                                .add_sized(
                                    Vec2::new(140.0 * sizes.zoom, sizes.button_height),
                                    slider,
                                )
                                .changed()
                            {
                                self.theme.set_zoom(self.zoom);
                            }

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "N/W to toggle density · Cmd/Ctrl ± for zoom",
                                    )
                                    .small()
                                    .color(self.theme.colors.text_dim),
                                );
                            });
                        });

                        ui.separator();

                        egui::ScrollArea::horizontal()
                            .id_source("mixer_skin_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, 0.0);
                                ui.horizontal_top(|ui| {
                                    for (index, strip) in self.strips.iter_mut().enumerate() {
                                        let alternate = index % 2 == 1;
                                        strip_widget(
                                            ui,
                                            strip,
                                            &self.theme,
                                            sizes,
                                            self.density,
                                            self.show_meter_bridge,
                                            alternate,
                                            false,
                                            callbacks,
                                        );
                                    }
                                    strip_widget(
                                        ui,
                                        &mut self.master,
                                        &self.theme,
                                        sizes,
                                        Density::Wide,
                                        self.show_meter_bridge,
                                        false,
                                        true,
                                        callbacks,
                                    );
                                });
                            });
                    });
            });
    }
}

fn strip_widget(
    ui: &mut Ui,
    strip: &mut StripModel,
    theme: &MixerTheme,
    sizes: ScaledSizes,
    density: Density,
    show_meter_bridge: bool,
    alternate: bool,
    is_master: bool,
    callbacks: &mut MixerCallbacks,
) {
    let width = if is_master {
        sizes.strip_width_wide
    } else {
        match density {
            Density::Narrow => sizes.strip_width_narrow,
            Density::Wide => sizes.strip_width_wide,
        }
    };

    let fill = if is_master {
        theme.colors.master_strip_bg
    } else if alternate {
        theme.colors.strip_bg_alt
    } else {
        theme.colors.strip_bg
    };

    Frame::none()
        .fill(fill)
        .stroke(theme.chrome_stroke)
        .rounding(Rounding::same(theme.rounding()))
        .inner_margin(Margin::symmetric(sizes.strip_inner_margin, sizes.spacing))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, sizes.spacing);

            let header_response = Frame::none()
                .fill(theme.colors.strip_header)
                .rounding(Rounding::same(theme.rounding() * 0.6))
                .inner_margin(Margin::symmetric(sizes.spacing * 0.6, sizes.spacing * 0.4))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing * 0.6, 0.0);
                    let (badge_rect, _badge_response) = ui.allocate_exact_size(
                        Vec2::new(10.0 * sizes.zoom, sizes.button_height * 0.9),
                        Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        badge_rect,
                        Rounding::same(theme.rounding() * 0.3),
                        theme.colors.button_badge,
                    );
                    let accent_rect = Rect::from_min_max(
                        badge_rect.left_top(),
                        Pos2::new(
                            badge_rect.left() + badge_rect.width() * 0.55,
                            badge_rect.bottom(),
                        ),
                    );
                    ui.painter().rect_filled(
                        accent_rect,
                        Rounding::same(theme.rounding() * 0.3),
                        strip.color,
                    );
                    ui.painter().rect_stroke(
                        badge_rect,
                        Rounding::same(theme.rounding() * 0.3),
                        Stroke::new(1.0, theme.chrome_stroke.color),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&strip.name)
                                .size(12.5 * sizes.zoom)
                                .color(theme.colors.text_primary),
                        )
                        .sense(Sense::click()),
                    )
                })
                .inner;
            if header_response.clicked() {
                strip.clip = false;
            }

            if show_meter_bridge {
                let bridge_height = sizes.fader_height * 0.35;
                let resp = meter_pair(ui, strip.meter_l, strip.meter_r, bridge_height, theme);
                theme.paint_peak_line(ui.painter(), resp.rect, strip.peak_l, true);
                theme.paint_peak_line(ui.painter(), resp.rect, strip.peak_r, false);
                theme.paint_clip_led(ui.painter(), resp.rect, strip.clip);
            }

            strip_rack(ui, strip, theme, sizes, density, callbacks);
            fader_strip(ui, strip, theme, sizes, density);

            let footer_height = sizes.button_height * 1.15;
            let (footer_rect, _) = ui.allocate_exact_size(
                Vec2::new(width - sizes.spacing * 0.4, footer_height),
                Sense::hover(),
            );
            ui.painter().rect_filled(
                footer_rect,
                Rounding::same(theme.rounding() * 0.45),
                strip.color,
            );
            ui.painter().rect_stroke(
                footer_rect,
                Rounding::same(theme.rounding() * 0.45),
                Stroke::new(1.0, theme.chrome_stroke.color),
            );
            ui.painter().text(
                footer_rect.center(),
                Align2::CENTER_CENTER,
                &strip.name,
                egui::FontId::proportional(11.0 * sizes.zoom),
                theme.colors.text_primary,
            );
        });
}

fn density_button(ui: &mut Ui, label: &str, active: bool, theme: &MixerTheme) -> Response {
    let sizes = theme.scaled_sizes();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(68.0 * sizes.zoom, sizes.button_height),
        Sense::click(),
    );
    let painter = ui.painter();
    let fill = if active {
        theme.colors.button_on
    } else {
        theme.colors.button_off
    };
    painter.rect(
        rect,
        Rounding::same(theme.rounding() * 0.3),
        fill,
        theme.chrome_stroke,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0 * sizes.zoom),
        theme.colors.text_primary,
    );
    response
}

fn strip_rack(
    ui: &mut Ui,
    strip: &mut StripModel,
    theme: &MixerTheme,
    sizes: ScaledSizes,
    density: Density,
    callbacks: &mut MixerCallbacks,
) {
    Frame::none()
        .fill(theme.colors.rack_panel)
        .rounding(Rounding::same(theme.rounding() * 0.6))
        .inner_margin(Margin::symmetric(sizes.spacing, sizes.spacing))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, sizes.spacing * 0.6);
            rack_section(ui, "INSERTS", theme, sizes);
            inserts_section(ui, strip, theme, sizes, density, callbacks);

            ui.add_space(sizes.spacing);

            rack_section(ui, "SENDS", theme, sizes);
            sends_section(ui, strip, theme, sizes, density, callbacks);
        });
}

#[derive(Clone, Debug)]
struct InsertDragPayload {
    channel: ChannelId,
    slot: usize,
}

#[derive(Clone, Debug)]
struct SendDragPayload {
    target: String,
}

fn insert_drag_id() -> egui::Id {
    egui::Id::new("mixer_skin_insert_drag")
}

fn send_drag_id() -> egui::Id {
    egui::Id::new("mixer_skin_send_drag")
}

fn current_insert_drag(ctx: &egui::Context) -> Option<InsertDragPayload> {
    ctx.data_mut(|data| data.get_temp(insert_drag_id()))
}

fn clear_insert_drag(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<InsertDragPayload>(insert_drag_id()));
}

fn set_insert_drag(ctx: &egui::Context, payload: InsertDragPayload) {
    ctx.data_mut(|data| data.insert_temp(insert_drag_id(), payload));
}

fn current_send_drag(ctx: &egui::Context) -> Option<SendDragPayload> {
    ctx.data_mut(|data| data.get_temp(send_drag_id()))
}

fn clear_send_drag(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<SendDragPayload>(send_drag_id()));
}

fn set_send_drag(ctx: &egui::Context, payload: SendDragPayload) {
    ctx.data_mut(|data| data.insert_temp(send_drag_id(), payload));
}

fn inserts_section(
    ui: &mut Ui,
    strip: &mut StripModel,
    theme: &MixerTheme,
    sizes: ScaledSizes,
    density: Density,
    callbacks: &mut MixerCallbacks,
) {
    let mut drop_request: Option<(InsertDragPayload, usize)> = None;
    let mut remove_request: Option<usize> = None;
    let slot_height = sizes.rack_row_height
        * match density {
            Density::Narrow => 1.0,
            Density::Wide => 1.1,
        };

    egui::ScrollArea::vertical()
        .id_source(("insert_section", strip.channel_id))
        .max_height(slot_height * MAX_RACK_SLOTS as f32 + sizes.spacing * 2.0)
        .show(ui, |ui| {
            for index in 0..MAX_RACK_SLOTS {
                let row_resp = Frame::none().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing =
                            Vec2::new(sizes.spacing * 0.45, sizes.spacing * 0.25);

                        let handle = ui.add_sized(
                            Vec2::new(16.0 * sizes.zoom, slot_height),
                            egui::Label::new("☰").sense(Sense::drag()),
                        );

                        if handle.drag_started() && index < strip.inserts.len() {
                            set_insert_drag(
                                ui.ctx(),
                                InsertDragPayload {
                                    channel: strip.channel_id,
                                    slot: index,
                                },
                            );
                        }
                        if handle.dragged() {
                            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
                        }

                        match strip.inserts.get_mut(index) {
                            Some(slot) => {
                                let toggle = rack_toggle(ui, &mut slot.on, theme, sizes, None);
                                if toggle.clicked() {
                                    (callbacks.set_insert_bypass)(
                                        strip.channel_id,
                                        index,
                                        !slot.on,
                                    );
                                }
                                toggle.on_hover_text("Toggle insert bypass");

                                let label_resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&slot.name)
                                            .size(11.2 * sizes.zoom)
                                            .color(theme.colors.text_primary),
                                    )
                                    .sense(Sense::click()),
                                );
                                if label_resp.double_clicked() {
                                    (callbacks.open_insert_ui)(strip.channel_id, index);
                                }

                                label_resp.context_menu(|ui| {
                                    if ui.button("Open").clicked() {
                                        (callbacks.open_insert_ui)(strip.channel_id, index);
                                        ui.close_menu();
                                    }
                                    if ui.button("Replace").clicked() {
                                        (callbacks.open_insert_browser)(
                                            strip.channel_id,
                                            Some(index),
                                        );
                                        ui.close_menu();
                                    }
                                    if ui.button("Remove").clicked() {
                                        remove_request = Some(index);
                                        ui.close_menu();
                                    }
                                    if ui.button("Add Insert").clicked() {
                                        (callbacks.open_insert_browser)(
                                            strip.channel_id,
                                            Some(index + 1),
                                        );
                                        ui.close_menu();
                                    }
                                });
                            }
                            None => {
                                let add_resp = ui.add_sized(
                                    Vec2::new(88.0 * sizes.zoom, slot_height),
                                    egui::Button::new("+ Add"),
                                );
                                if add_resp.clicked() {
                                    (callbacks.open_insert_browser)(strip.channel_id, Some(index));
                                }
                            }
                        }
                    });
                });

                if let Some(payload) = current_insert_drag(ui.ctx()) {
                    let hovering = ui.rect_contains_pointer(row_resp.response.rect);
                    if hovering {
                        ui.painter().rect_stroke(
                            row_resp.response.rect,
                            Rounding::same(theme.rounding() * 0.2),
                            Stroke::new(1.1, theme.colors.button_badge),
                        );

                        if ui.input(|i| i.pointer.any_released()) {
                            drop_request = Some((payload, index));
                        }
                    }
                }
            }
        });

    if let Some(idx) = remove_request {
        if idx < strip.inserts.len() {
            strip.inserts.remove(idx);
            (callbacks.remove_insert)(strip.channel_id, idx);
        }
    }

    if let Some((payload, target)) = drop_request {
        clear_insert_drag(ui.ctx());
        if payload.channel == strip.channel_id {
            if payload.slot < strip.inserts.len() {
                let slot = strip.inserts.remove(payload.slot);
                let mut dest = target.min(strip.inserts.len());
                if dest > payload.slot {
                    dest = dest.saturating_sub(1);
                }
                dest = dest.min(strip.inserts.len());
                strip.inserts.insert(dest, slot);
                (callbacks.reorder_insert)(strip.channel_id, payload.slot, dest);
            }
        } else {
            (callbacks.open_insert_browser)(strip.channel_id, Some(target));
        }
    }
}

fn sends_section(
    ui: &mut Ui,
    strip: &mut StripModel,
    theme: &MixerTheme,
    sizes: ScaledSizes,
    density: Density,
    callbacks: &mut MixerCallbacks,
) {
    let slot_height = sizes.rack_row_height
        * match density {
            Density::Narrow => 1.0,
            Density::Wide => 1.1,
        };
    let mut drop_request: Option<(SendDragPayload, usize)> = None;

    egui::ScrollArea::vertical()
        .id_source(("sends_section", strip.channel_id))
        .max_height(slot_height * MAX_RACK_SLOTS as f32 + sizes.spacing * 2.0)
        .show(ui, |ui| {
            for index in 0..MAX_RACK_SLOTS {
                let row_resp = Frame::none().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing =
                            Vec2::new(sizes.spacing * 0.45, sizes.spacing * 0.2);

                        if let Some(send) = strip.sends.get_mut(index) {
                            let toggle = rack_toggle(ui, &mut send.on, theme, sizes, None);
                            if toggle.clicked() {
                                let level = if send.on { send.gain } else { 0.0 };
                                (callbacks.configure_send)(
                                    strip.channel_id,
                                    send.id,
                                    level,
                                    send.pre,
                                );
                            }
                            toggle.on_hover_text("Toggle send");

                            let tag_label = if send.pre { "PRE" } else { "POST" };
                            let mut badge = ui.add_sized(
                                Vec2::new(38.0 * sizes.zoom, slot_height),
                                egui::SelectableLabel::new(send.pre, tag_label),
                            );
                            if badge.clicked() {
                                send.pre = !send.pre;
                                (callbacks.configure_send)(
                                    strip.channel_id,
                                    send.id,
                                    send.gain,
                                    send.pre,
                                );
                            }
                            badge.on_hover_text("Toggle pre/post");

                            let drag_handle = ui.add_sized(
                                Vec2::new(18.0 * sizes.zoom, slot_height),
                                egui::Label::new("↕").sense(Sense::drag()),
                            );
                            if drag_handle.drag_started() {
                                set_send_drag(
                                    ui.ctx(),
                                    SendDragPayload {
                                        target: send.dest.clone(),
                                    },
                                );
                            }
                            if drag_handle.dragged() {
                                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
                            }

                            let dest_resp = egui::ComboBox::from_id_source((
                                strip.channel_id,
                                "send_dest",
                                index,
                            ))
                            .selected_text(send.dest.clone())
                            .show_ui(ui, |ui| {
                                for slot in 0..MAX_RACK_SLOTS {
                                    let label = format!("Bus {}", (b'A' + slot as u8) as char);
                                    let selected = send.dest == label;
                                    if ui.selectable_label(selected, &label).clicked() {
                                        send.dest = label;
                                        (callbacks.configure_send)(
                                            strip.channel_id,
                                            send.id,
                                            send.gain,
                                            send.pre,
                                        );
                                        ui.close_menu();
                                    }
                                }
                            })
                            .response;

                            if dest_resp.hovered() {
                                if let Some(payload) = current_send_drag(ui.ctx()) {
                                    if ui.input(|i| i.pointer.any_released()) {
                                        send.dest = payload.target;
                                        (callbacks.configure_send)(
                                            strip.channel_id,
                                            send.id,
                                            send.gain,
                                            send.pre,
                                        );
                                        clear_send_drag(ui.ctx());
                                    }
                                }
                            }

                            let mut level = send.gain;
                            let slider = egui::Slider::new(&mut level, 0.0..=1.0)
                                .show_value(false)
                                .step_by(0.01)
                                .clamp_to_range(true);
                            let resp = ui.add_sized(
                                Vec2::new(80.0 * sizes.zoom, sizes.spacing * 3.0),
                                slider,
                            );
                            if resp.changed() {
                                send.gain = level;
                                let level = if send.on { send.gain } else { 0.0 };
                                (callbacks.configure_send)(
                                    strip.channel_id,
                                    send.id,
                                    level,
                                    send.pre,
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!("{:.0}%", send.gain * 100.0))
                                    .small()
                                    .color(theme.colors.text_dim),
                            );
                        } else {
                            let add_resp = ui.add_sized(
                                Vec2::new(110.0 * sizes.zoom, slot_height),
                                egui::Button::new("+ Add Send"),
                            );
                            if add_resp.clicked() {
                                strip.sends.push(SendSlot {
                                    id: index as SendId,
                                    dest: format!("Bus {}", (b'A' + index as u8) as char),
                                    gain: 0.0,
                                    pre: false,
                                    on: true,
                                });
                                (callbacks.configure_send)(
                                    strip.channel_id,
                                    index as SendId,
                                    0.0,
                                    false,
                                );
                            }

                            if let Some(payload) = current_send_drag(ui.ctx()) {
                                if add_resp.hovered() && ui.input(|i| i.pointer.any_released()) {
                                    drop_request = Some((payload, index));
                                }
                            }
                        }
                    });
                });

                if let Some(payload) = current_send_drag(ui.ctx()) {
                    if ui.rect_contains_pointer(row_resp.response.rect)
                        && ui.input(|i| i.pointer.any_released())
                    {
                        drop_request = Some((payload, index));
                    }
                }
            }
        });

    if let Some((payload, index)) = drop_request {
        if index < strip.sends.len() {
            strip.sends[index].dest = payload.target.clone();
            let level = if strip.sends[index].on {
                strip.sends[index].gain
            } else {
                0.0
            };
            (callbacks.configure_send)(
                strip.channel_id,
                strip.sends[index].id,
                level,
                strip.sends[index].pre,
            );
        } else if index < MAX_RACK_SLOTS {
            strip.sends.push(SendSlot {
                id: index as SendId,
                dest: payload.target,
                gain: 0.0,
                pre: false,
                on: true,
            });
            (callbacks.configure_send)(strip.channel_id, index as SendId, 0.0, false);
        }
        clear_send_drag(ui.ctx());
    }
}

fn rack_section(ui: &mut Ui, title: &str, theme: &MixerTheme, sizes: ScaledSizes) {
    ui.label(
        egui::RichText::new(title)
            .size(11.5 * sizes.zoom)
            .color(theme.colors.rack_header),
    );
}

fn rack_toggle(
    ui: &mut Ui,
    on: &mut bool,
    theme: &MixerTheme,
    sizes: ScaledSizes,
    label: Option<&str>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(46.0 * sizes.zoom, sizes.rack_row_height),
        Sense::click(),
    );
    if response.clicked() {
        *on = !*on;
    }
    let painter = ui.painter();
    let fill = if *on {
        theme.colors.button_on
    } else {
        theme.colors.button_off
    };
    painter.rect(
        rect,
        Rounding::same(theme.rounding() * 0.25),
        fill,
        theme.chrome_stroke,
    );
    if let Some(text) = label {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(10.0 * sizes.zoom),
            theme.colors.text_primary,
        );
    }
    response
}

fn fader_strip(
    ui: &mut Ui,
    strip: &mut StripModel,
    theme: &MixerTheme,
    sizes: ScaledSizes,
    density: Density,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, sizes.spacing * 0.6);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, 0.0);
            toggle_button(ui, "R", &mut strip.rec, theme, sizes).on_hover_text("Record arm");
            toggle_button(ui, "S", &mut strip.solo, theme, sizes).on_hover_text("Solo");
            toggle_button(ui, "M", &mut strip.mute, theme, sizes).on_hover_text("Mute");
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, 0.0);
            knob_small(ui, "PAN", &mut strip.pan, -1.0, 1.0, theme).on_hover_text("Pan");
            knob_small(ui, "WID", &mut strip.width, 0.0, 2.0, theme).on_hover_text("Stereo width");
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, 0.0);
            let meter_resp =
                meter_pair(ui, strip.meter_l, strip.meter_r, sizes.fader_height, theme);
            theme.paint_peak_line(ui.painter(), meter_resp.rect, strip.peak_l, true);
            theme.paint_peak_line(ui.painter(), meter_resp.rect, strip.peak_r, false);
            theme.paint_clip_led(ui.painter(), meter_resp.rect, strip.clip);

            let response = fader(ui, &mut strip.gain_db, sizes.fader_height, theme);
            if response.changed() {
                strip.gain_db = strip.gain_db.clamp(DB_MIN, DB_MAX);
            }
        });

        let gain_text = if (strip.gain_db).abs() < 0.05 {
            "0.0 dB".to_owned()
        } else {
            format!("{:+.1} dB", strip.gain_db)
        };
        ui.label(
            egui::RichText::new(gain_text)
                .small()
                .color(theme.colors.text_dim),
        );
    });

    if matches!(density, Density::Wide) {
        ui.add_space(sizes.spacing * 0.5);
    }
}

pub fn fader(ui: &mut Ui, gain_db: &mut f32, height: f32, theme: &MixerTheme) -> Response {
    let sizes = theme.scaled_sizes();
    let width = sizes.fader_width;
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let painter = ui.painter();

    let min_gain = db_to_gain(DB_MIN);
    let max_gain = db_to_gain(DB_MAX);
    let mut norm = gain_to_norm(*gain_db, min_gain, max_gain);

    if response.dragged() {
        let delta = ui.ctx().input(|i| i.pointer.delta().y);
        norm -= delta / rect.height();
        norm = norm.clamp(0.0, 1.0);
        *gain_db = norm_to_db(norm, min_gain, max_gain);
        response.mark_changed();
        ui.ctx().request_repaint();
    }

    if response.double_clicked() || response.secondary_clicked() {
        *gain_db = 0.0;
        norm = gain_to_norm(*gain_db, min_gain, max_gain);
        response.mark_changed();
    }

    painter.rect_filled(
        rect,
        Rounding::same(theme.rounding() * 0.4),
        theme.colors.fader_track,
    );
    painter.rect_stroke(
        rect,
        Rounding::same(theme.rounding() * 0.4),
        theme.chrome_stroke,
    );
    let inner = rect.shrink2(Vec2::new(width * 0.18, width * 0.25));
    painter.rect_filled(
        inner,
        Rounding::same(theme.rounding() * 0.35),
        theme.colors.fader_track_inner,
    );

    const TICKS: [f32; 10] = [
        -60.0, -36.0, -24.0, -18.0, -12.0, -6.0, -3.0, 0.0, 6.0, 12.0,
    ];
    for tick in TICKS {
        let y = rect.bottom() - gain_to_norm(tick, min_gain, max_gain) * rect.height();
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.left() + 6.0, y)],
            Stroke::new(1.0, theme.colors.text_dim),
        );
        if (tick as i32) % 12 == 0 {
            painter.text(
                Pos2::new(rect.left() - 3.0, y),
                Align2::RIGHT_CENTER,
                format!("{:+}", tick as i32),
                egui::FontId::proportional(9.5 * sizes.zoom),
                theme.colors.text_dim,
            );
        }
    }

    let handle_height = 14.0 * sizes.zoom;
    let handle_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.bottom() - norm * rect.height()),
        Vec2::new(width * 0.7, handle_height),
    );
    let handle_rounding = Rounding::same(handle_height * 0.4);
    painter.rect_filled(handle_rect, handle_rounding, theme.colors.fader_handle);
    painter.rect_stroke(
        handle_rect,
        handle_rounding,
        Stroke::new(1.0, theme.colors.meter_border.gamma_multiply(0.8)),
    );
    let indicator = Rect::from_center_size(
        handle_rect.center(),
        Vec2::new(handle_rect.width() * 0.35, handle_height * 0.35),
    );
    painter.rect_filled(
        indicator,
        Rounding::same(handle_height * 0.2),
        theme.colors.button_on,
    );

    response
}

pub fn knob_small(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    theme: &MixerTheme,
) -> Response {
    let sizes = theme.scaled_sizes();
    let diameter = sizes.knob_diameter;
    let total_size = Vec2::new(diameter, diameter + sizes.spacing * 3.0);
    let (rect, mut response) = ui.allocate_exact_size(total_size, Sense::click_and_drag());

    if response.dragged() {
        let delta = ui.ctx().input(|i| i.pointer.delta().y);
        let range = max - min;
        let sensitivity = (range * 0.003).max(0.001);
        *value = (*value - delta * sensitivity).clamp(min, max);
        response.mark_changed();
        ui.ctx().request_repaint();
    }
    if response.double_clicked() {
        *value = ((min + max) * 0.5).clamp(min, max);
        response.mark_changed();
    }

    let knob_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + diameter * 0.5),
        Vec2::splat(diameter),
    );
    let painter = ui.painter();
    painter.circle_filled(
        knob_rect.center(),
        diameter * 0.5,
        theme.colors.fader_track_inner,
    );
    painter.circle_stroke(knob_rect.center(), diameter * 0.5, theme.chrome_stroke);

    let norm = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let start_angle = std::f32::consts::PI * 1.25;
    let end_angle = std::f32::consts::PI * -0.25;
    let angle = start_angle + norm * (end_angle - start_angle);
    let radius = diameter * 0.45;
    let indicator = Pos2::new(
        knob_rect.center().x + radius * angle.cos(),
        knob_rect.center().y + radius * angle.sin(),
    );
    painter.line_segment(
        [knob_rect.center(), indicator],
        Stroke::new(2.0, theme.colors.button_on),
    );

    painter.text(
        Pos2::new(rect.center().x, rect.bottom()),
        Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.0 * sizes.zoom),
        theme.colors.text_dim,
    );

    response
}

pub fn meter_pair(ui: &mut Ui, l: f32, r: f32, height: f32, theme: &MixerTheme) -> Response {
    let sizes = theme.scaled_sizes();
    let width = sizes.meter_width;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        Rounding::same(theme.rounding() * 0.35),
        theme.colors.meter_background,
    );
    painter.rect_stroke(
        rect,
        Rounding::same(theme.rounding() * 0.35),
        Stroke::new(1.0, theme.colors.meter_border),
    );

    let divider_x = rect.center().x;
    painter.line_segment(
        [
            Pos2::new(divider_x, rect.top() + 3.0),
            Pos2::new(divider_x, rect.bottom() - 3.0),
        ],
        Stroke::new(1.0, theme.colors.meter_border.gamma_multiply(0.9)),
    );

    let gutter = 3.0 * theme.zoom();
    let usable_height = rect.height() - gutter * 2.0;
    for tick in [0.25_f32, 0.5, 0.75] {
        let y = rect.bottom() - gutter - tick * usable_height;
        painter.line_segment(
            [
                Pos2::new(rect.left() + gutter * 0.6, y),
                Pos2::new(rect.right() - gutter * 0.6, y),
            ],
            Stroke::new(0.8, theme.colors.meter_grid.gamma_multiply(0.6)),
        );
    }
    theme.paint_meter(painter, rect, l, true);
    theme.paint_meter(painter, rect, r, false);
    response
}

fn toggle_button(
    ui: &mut Ui,
    label: &str,
    value: &mut bool,
    theme: &MixerTheme,
    sizes: ScaledSizes,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(30.0 * sizes.zoom, sizes.button_height),
        Sense::click(),
    );
    if response.clicked() {
        *value = !*value;
    }
    let fill = if *value {
        match label {
            "M" => theme.colors.button_mute_on,
            "S" => theme.colors.button_solo_on,
            "R" => theme.colors.button_rec_on,
            _ => theme.colors.button_on,
        }
    } else {
        theme.colors.button_off
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        Rounding::same(theme.rounding() * 0.25),
        fill,
        theme.chrome_stroke,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.5 * sizes.zoom),
        theme.colors.text_primary,
    );
    response
}

fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

fn gain_to_norm(db: f32, min_gain: f32, max_gain: f32) -> f32 {
    let gain = db_to_gain(db.clamp(DB_MIN, DB_MAX));
    ((gain - min_gain) / (max_gain - min_gain)).clamp(0.0, 1.0)
}

fn norm_to_db(norm: f32, min_gain: f32, max_gain: f32) -> f32 {
    let gain = min_gain + norm.clamp(0.0, 1.0) * (max_gain - min_gain);
    20.0 * gain.log10()
}

#[cfg(feature = "demo-app")]
pub fn run_demo() {
    use eframe::{egui, NativeOptions};

    struct DemoApp {
        mixer: MixerUi,
        callbacks: MixerCallbacks,
    }

    impl eframe::App for DemoApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.mixer.ui(ui, &mut self.callbacks);
            });
        }
    }

    let theme = MixerTheme::default();
    let strips = (0..12)
        .map(|i| StripModel {
            channel_id: i as ChannelId + 1,
            name: format!("CH {:02}", i + 1),
            color: Color32::from_rgb(0x46, 0x50 + (i as u8 * 3), 0x5A + (i as u8 * 2)),
            meter_l: (0.25 + 0.03 * i as f32).fract(),
            meter_r: (0.35 + 0.028 * i as f32).fract(),
            peak_l: 0.4 + 0.02 * (i as f32 % 10.0),
            peak_r: 0.45 + 0.015 * (i as f32 % 11.0),
            clip: i % 7 == 0,
            gain_db: -6.0 + i as f32 * 0.5,
            pan: ((i as f32 * 0.35).sin()).clamp(-1.0, 1.0),
            width: 1.0 + ((i as f32 * 0.21).cos() * 0.5),
            mute: i % 5 == 0,
            solo: i % 4 == 0,
            rec: i % 3 == 0,
            inserts: (0..5)
                .map(|n| Slot {
                    name: format!("Insert {}", n + 1),
                    on: n % 2 == 0,
                })
                .collect(),
            sends: (0..4)
                .map(|n| SendSlot {
                    id: n as SendId,
                    dest: format!("Bus {}", (b'A' + n as u8) as char),
                    gain: 0.45 + 0.1 * n as f32,
                    pre: n % 2 == 0,
                    on: true,
                })
                .collect(),
        })
        .collect();

    let master = StripModel {
        channel_id: 0,
        name: "MASTER".into(),
        color: Color32::from_rgb(0x64, 0x80, 0x90),
        meter_l: 0.82,
        meter_r: 0.78,
        peak_l: 0.92,
        peak_r: 0.94,
        clip: true,
        gain_db: -0.5,
        pan: 0.0,
        width: 1.0,
        mute: false,
        solo: false,
        rec: false,
        inserts: vec![
            Slot {
                name: "Bus Comp".into(),
                on: true,
            },
            Slot {
                name: "Limiter".into(),
                on: true,
            },
        ],
        sends: vec![SendSlot {
            id: 0,
            dest: "Phones".into(),
            gain: 0.7,
            pre: false,
            on: true,
        }],
    };

    let callbacks = MixerCallbacks::noop();

    let mixer = MixerUi {
        strips,
        master,
        density: Density::Narrow,
        zoom: 1.0,
        show_meter_bridge: true,
        theme,
    };

    let options = NativeOptions::default();
    let _ = eframe::run_native(
        "Mixer Skin Demo",
        options,
        Box::new(move |_| Box::new(DemoApp { mixer, callbacks })),
    );
}
