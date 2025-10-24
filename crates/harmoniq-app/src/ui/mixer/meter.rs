use std::time::Instant;

use eframe::egui::{pos2, Color32, Id, Painter, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::ui::mixer::theme::MixerTheme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterLevels {
    pub left_peak: f32,
    pub right_peak: f32,
    pub left_true_peak: f32,
    pub right_true_peak: f32,
    pub clipped: bool,
}

impl MeterLevels {
    pub fn silence() -> Self {
        Self {
            left_peak: f32::NEG_INFINITY,
            right_peak: f32::NEG_INFINITY,
            left_true_peak: f32::NEG_INFINITY,
            right_true_peak: f32::NEG_INFINITY,
            clipped: false,
        }
    }
}

#[derive(Debug)]
pub struct MeterState {
    pub last_levels: MeterLevels,
    hold_levels: [f32; 2],
    hold_time: [f32; 2],
    hold_decay: f32,
    last_update: Instant,
    clip_latched: bool,
}

impl Default for MeterState {
    fn default() -> Self {
        Self {
            last_levels: MeterLevels::silence(),
            hold_levels: [f32::NEG_INFINITY; 2],
            hold_time: [0.0; 2],
            hold_decay: 1.5,
            last_update: Instant::now(),
            clip_latched: false,
        }
    }
}

impl MeterState {
    pub fn update(&mut self, new_levels: MeterLevels) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;
        self.last_levels = new_levels;
        if new_levels.clipped {
            self.clip_latched = true;
        }

        for (channel, peak) in [new_levels.left_peak, new_levels.right_peak]
            .into_iter()
            .enumerate()
        {
            if peak > self.hold_levels[channel] {
                self.hold_levels[channel] = peak;
                self.hold_time[channel] = 0.0;
            } else {
                self.hold_time[channel] += dt;
                let decay = (self.hold_time[channel] / self.hold_decay).clamp(0.0, 1.0);
                self.hold_levels[channel] = lerp_db(self.hold_levels[channel], peak, decay);
            }
        }
    }

    pub fn hold_levels(&self) -> [f32; 2] {
        self.hold_levels
    }

    pub fn clear_clip(&mut self) {
        self.clip_latched = false;
    }

    pub fn clip_latched(&self) -> bool {
        self.clip_latched
    }
}

#[cfg(test)]
impl MeterState {
    pub(crate) fn set_last_update_for_test(&mut self, instant: Instant) {
        self.last_update = instant;
    }
}

pub fn paint_meter(
    ui: &mut Ui,
    rect: Rect,
    meter_id: Id,
    state: &mut MeterState,
    theme: &MixerTheme,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme.rounding_small, theme.meter_bg);

    let channel_width = rect.width() * 0.5 - 1.0;
    let left_rect = Rect::from_min_max(rect.min, pos2(rect.min.x + channel_width, rect.max.y));
    let right_rect = Rect::from_min_max(pos2(rect.max.x - channel_width, rect.min.y), rect.max);

    paint_channel(
        &painter,
        left_rect,
        state.last_levels.left_peak,
        state.last_levels.left_true_peak,
        state.hold_levels[0],
        theme,
    );
    paint_channel(
        &painter,
        right_rect,
        state.last_levels.right_peak,
        state.last_levels.right_true_peak,
        state.hold_levels[1],
        theme,
    );

    let clip_rect = Rect::from_min_size(
        pos2(rect.center().x - 7.0, rect.min.y - 14.0),
        Vec2::new(14.0, 10.0),
    );
    let clip_color = if state.clip_latched() {
        theme.clip
    } else {
        theme.icon_bg
    };
    painter.rect_filled(clip_rect, theme.rounding_small, clip_color);

    let clip_id = meter_id.with("clip");
    if ui.interact(clip_rect, clip_id, Sense::click()).clicked() {
        state.clear_clip();
    }
}

fn paint_channel(
    painter: &Painter,
    rect: Rect,
    peak_db: f32,
    true_peak_db: f32,
    hold_db: f32,
    theme: &MixerTheme,
) {
    let peak_ratio = db_to_ratio(peak_db);
    let true_ratio = db_to_ratio(true_peak_db);
    let hold_ratio = db_to_ratio(hold_db);

    let peak_height = rect.height() * peak_ratio;
    let peak_rect = Rect::from_min_max(pos2(rect.min.x, rect.max.y - peak_height), rect.max);
    if peak_height > 0.0 {
        let top_color = meter_color_for_db(peak_db, theme);
        let bottom_color = theme.meter_bg.linear_multiply(0.4);
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(peak_rect.left_top(), top_color);
        mesh.colored_vertex(peak_rect.right_top(), top_color);
        mesh.colored_vertex(peak_rect.right_bottom(), bottom_color);
        mesh.colored_vertex(peak_rect.left_bottom(), bottom_color);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
    }

    if hold_ratio > 0.0 {
        let y = rect.max.y - rect.height() * hold_ratio;
        painter.line_segment(
            [pos2(rect.min.x, y), pos2(rect.max.x, y)],
            Stroke::new(1.5, theme.meter_true_peak),
        );
    }

    if true_ratio > 0.0 {
        let y = rect.max.y - rect.height() * true_ratio;
        painter.line_segment(
            [pos2(rect.min.x, y), pos2(rect.max.x, y)],
            Stroke::new(1.0, meter_color_for_db(true_peak_db, theme)),
        );
    }
}

fn db_to_ratio(db: f32) -> f32 {
    if db <= -90.0 {
        0.0
    } else {
        (10.0f32.powf(db / 20.0)).clamp(0.0, 1.0)
    }
}

fn meter_color_for_db(db: f32, theme: &MixerTheme) -> Color32 {
    let clamped = db.clamp(-60.0, 6.0);
    if clamped >= -1.0 {
        theme.meter_red
    } else if clamped >= -6.0 {
        let t = (clamped + 6.0) / 5.0;
        lerp_color(theme.meter_yellow, theme.meter_red, t)
    } else {
        let t = (clamped + 60.0) / 54.0;
        lerp_color(theme.meter_green, theme.meter_yellow, t)
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let r = a.r() as f32 + (b.r() as f32 - a.r() as f32) * t;
    let g = a.g() as f32 + (b.g() as f32 - a.g() as f32) * t;
    let b = a.b() as f32 + (b.b() as f32 - a.b() as f32) * t;
    Color32::from_rgb(r as u8, g as u8, b as u8)
}

fn lerp_db(a: f32, b: f32, t: f32) -> f32 {
    let a_lin = db_to_ratio(a);
    let b_lin = db_to_ratio(b);
    let mixed = a_lin * (1.0 - t) + b_lin * t;
    if mixed <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * mixed.log10()
    }
}
