use egui::{
    self, Align, Align2, Color32, Frame, Layout, Margin, Pos2, Rect, Response, Rounding, Sense,
    Stroke, Ui, Vec2,
};

const DB_MIN: f32 = -60.0;
const DB_MAX: f32 = 12.0;

/// Insert slot entry metadata for the rack section.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    pub on: bool,
}

/// Send slot entry metadata for the rack section.
#[derive(Clone, Debug, PartialEq)]
pub struct SendSlot {
    pub dest: String,
    pub gain: f32,
    pub pre: bool,
    pub on: bool,
}

/// Mixer strip data model consumed by the UI skin.
#[derive(Clone, Debug, PartialEq)]
pub struct StripModel {
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
    pub meter_bar: Color32,
    pub meter_clip: Color32,
    pub text_primary: Color32,
    pub text_dim: Color32,
    pub button_on: Color32,
    pub button_off: Color32,
    pub rack_header: Color32,
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
                background: Color32::from_rgb(0x12, 0x16, 0x1A),
                panel: Color32::from_rgb(0x1C, 0x22, 0x28),
                strip_bg: Color32::from_rgb(0x22, 0x2A, 0x33),
                strip_bg_alt: Color32::from_rgb(0x29, 0x32, 0x3B),
                master_strip_bg: Color32::from_rgb(0x27, 0x39, 0x46),
                meter_bar: Color32::from_rgb(0x00, 0xEB, 0xD4),
                meter_clip: Color32::from_rgb(0xFF, 0x6B, 0x6B),
                text_primary: Color32::from_gray(240),
                text_dim: Color32::from_rgb(0xBF, 0xD0, 0xDF),
                button_on: Color32::from_rgb(0x46, 0xC8, 0xFF),
                button_off: Color32::from_rgb(0x46, 0x50, 0x5A),
                rack_header: Color32::from_rgb(0xB4, 0xC7, 0xD4),
            },
            sizes: ThemeSizes {
                strip_width_narrow: 74.0,
                strip_width_wide: 110.0,
                fader_height: 220.0,
                fader_width: 28.0,
                meter_width: 12.0,
                knob_diameter: 34.0,
                spacing: 4.0,
                inner_spacing: 6.0,
                rack_row_height: 20.0,
                rack_section_title: 14.0,
                button_height: 18.0,
                strip_inner_margin: 8.0,
            },
            zoom: 1.0,
            rounding: 8.0,
            chrome_stroke: Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 255)),
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
        let clamped = value.clamp(0.0, 1.0);
        let half_width = rect.width() * 0.5 - 1.0;
        let x_min = if left {
            rect.left() + 1.0
        } else {
            rect.center().x + 1.0
        };
        let x_max = x_min + half_width;
        let y_min = rect.bottom() - clamped * rect.height();
        let bar_rect = Rect::from_min_max(
            Pos2::new(x_min, y_min),
            Pos2::new(x_max, rect.bottom() - 1.0),
        );
        painter.rect_filled(
            bar_rect,
            Rounding::same(self.rounding() * 0.15),
            self.colors.meter_bar,
        );
    }

    /// Paint peak hold line overlay for a single meter.
    pub fn paint_peak_line(&self, painter: &egui::Painter, rect: Rect, peak: f32, left: bool) {
        let clamped = peak.clamp(0.0, 1.0);
        let half_width = rect.width() * 0.5 - 1.0;
        let x_min = if left {
            rect.left() + 1.0
        } else {
            rect.center().x + 1.0
        };
        let y = rect.bottom() - clamped * rect.height();
        painter.line_segment(
            [Pos2::new(x_min, y), Pos2::new(x_min + half_width, y)],
            Stroke::new(1.0, self.colors.text_dim),
        );
    }

    /// Paint a clip LED at the top of the meter pair.
    pub fn paint_clip_led(&self, painter: &egui::Painter, rect: Rect, clip: bool) {
        let led_size = (rect.width() * 0.5).clamp(6.0, 9.0);
        let led_rect = Rect::from_center_size(
            Pos2::new(rect.center().x, rect.top() + led_size * 0.75),
            Vec2::splat(led_size),
        );
        let color = if clip {
            self.colors.meter_clip
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 180)
        };
        painter.rect_filled(led_rect, Rounding::same(led_size * 0.4), color);
        painter.rect_stroke(
            led_rect,
            Rounding::same(led_size * 0.4),
            Stroke::new(1.0, self.chrome_stroke.color),
        );
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
    pub fn ui(&mut self, ui: &mut egui::Ui) {
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
                if input.key_pressed(egui::Key::PlusEquals) {
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
            .fill(self.theme.colors.panel)
            .rounding(Rounding::same(self.theme.rounding()))
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
                        .add_sized(Vec2::new(140.0 * sizes.zoom, sizes.button_height), slider)
                        .changed()
                    {
                        self.theme.set_zoom(self.zoom);
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("N/W to toggle density · Cmd/Ctrl ± for zoom")
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
                            );
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

            let header_response = ui
                .horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing * 0.6, 0.0);
                    let badge = ui.allocate_exact_size(
                        Vec2::new(8.0 * sizes.zoom, sizes.button_height * 0.8),
                        Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        badge.rect,
                        Rounding::same(theme.rounding() * 0.2),
                        strip.color,
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&strip.name)
                                .size(12.0 * sizes.zoom)
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

            strip_rack(ui, strip, theme, sizes, density);
            fader_strip(ui, strip, theme, sizes, density);
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
) {
    let max_inserts = match density {
        Density::Narrow => 3,
        Density::Wide => 5,
    };
    let max_sends = match density {
        Density::Narrow => 2,
        Density::Wide => 4,
    };

    Frame::none()
        .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 26))
        .rounding(Rounding::same(theme.rounding() * 0.6))
        .inner_margin(Margin::symmetric(sizes.spacing, sizes.spacing))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing, sizes.spacing * 0.6);
            rack_section(ui, "INSERTS", theme, sizes);
            for slot in strip.inserts.iter_mut().take(max_inserts) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing * 0.5, 0.0);
                    let toggle =
                        rack_toggle(ui, &mut slot.on, theme, sizes, Some(slot.name.as_str()));
                    toggle.on_hover_text("Toggle insert");
                    ui.label(
                        egui::RichText::new(&slot.name)
                            .size(11.2 * sizes.zoom)
                            .color(theme.colors.text_primary),
                    );
                });
            }
            if strip.inserts.len() > max_inserts {
                ui.label(
                    egui::RichText::new(format!("+{} more…", strip.inserts.len() - max_inserts))
                        .small()
                        .color(theme.colors.text_dim),
                );
            }

            ui.add_space(sizes.spacing);

            rack_section(ui, "SENDS", theme, sizes);
            for slot in strip.sends.iter_mut().take(max_sends) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sizes.spacing * 0.5, 0.0);
                    let toggle = rack_toggle(ui, &mut slot.on, theme, sizes, None);
                    toggle.on_hover_text("Toggle send");

                    let tag = if slot.pre { "PRE" } else { "POST" };
                    let tag_rect = ui.allocate_exact_size(
                        Vec2::new(34.0 * sizes.zoom, sizes.rack_row_height),
                        Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        tag_rect.rect,
                        Rounding::same(theme.rounding() * 0.2),
                        Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                    );
                    ui.painter().text(
                        tag_rect.rect.center(),
                        Align2::CENTER_CENTER,
                        tag,
                        egui::FontId::proportional(10.0 * sizes.zoom),
                        theme.colors.text_dim,
                    );

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, sizes.spacing * 0.3);
                        ui.label(
                            egui::RichText::new(&slot.dest)
                                .size(11.0 * sizes.zoom)
                                .color(theme.colors.text_primary),
                        );
                        let slider = egui::Slider::new(&mut slot.gain, 0.0..=1.0)
                            .logarithmic(true)
                            .show_value(false)
                            .step_by(0.01);
                        ui.add_sized(Vec2::new(70.0 * sizes.zoom, sizes.spacing * 3.4), slider);
                    });
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", slot.gain * 100.0))
                            .small()
                            .color(theme.colors.text_dim),
                    );
                });
            }
            if strip.sends.len() > max_sends {
                ui.label(
                    egui::RichText::new(format!("+{} more…", strip.sends.len() - max_sends))
                        .small()
                        .color(theme.colors.text_dim),
                );
            }
        });
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
        Color32::from_rgb(0x18, 0x1F, 0x25),
    );
    painter.rect_stroke(
        rect,
        Rounding::same(theme.rounding() * 0.4),
        theme.chrome_stroke,
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
    painter.rect_filled(
        handle_rect,
        Rounding::same(handle_height * 0.4),
        theme.colors.button_on,
    );
    painter.rect_stroke(
        handle_rect,
        Rounding::same(handle_height * 0.4),
        theme.chrome_stroke,
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
        Color32::from_rgb(0x1A, 0x21, 0x27),
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
        Rounding::same(theme.rounding() * 0.3),
        Color32::from_rgb(0x18, 0x1F, 0x25),
    );
    painter.rect_stroke(
        rect,
        Rounding::same(theme.rounding() * 0.3),
        theme.chrome_stroke,
    );
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
        theme.colors.button_on
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
    }

    impl eframe::App for DemoApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.mixer.ui(ui);
            });
        }
    }

    let theme = MixerTheme::default();
    let strips = (0..12)
        .map(|i| StripModel {
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
                    dest: format!("Bus {}", (b'A' + n as u8) as char),
                    gain: 0.45 + 0.1 * n as f32,
                    pre: n % 2 == 0,
                    on: true,
                })
                .collect(),
        })
        .collect();

    let master = StripModel {
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
            dest: "Phones".into(),
            gain: 0.7,
            pre: false,
            on: true,
        }],
    };

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
        Box::new(move |_| Box::new(DemoApp { mixer })),
    );
}
