use std::time::Instant;

use eframe::egui::{pos2, Color32, Painter, Rect, Vec2};

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
}

impl Default for MeterState {
    fn default() -> Self {
        Self {
            last_levels: MeterLevels::silence(),
            hold_levels: [f32::NEG_INFINITY; 2],
            hold_time: [0.0; 2],
            hold_decay: 1.5,
            last_update: Instant::now(),
        }
    }
}

impl MeterState {
    pub fn update(&mut self, new_levels: MeterLevels) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;
        self.last_levels = new_levels;

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
                let hold_linear = 10.0f32.powf(self.hold_levels[channel] / 20.0);
                let peak_linear = 10.0f32.powf(peak / 20.0);
                let mixed = hold_linear * (1.0 - decay) + peak_linear * decay;
                self.hold_levels[channel] = if mixed.abs() < f32::EPSILON {
                    f32::NEG_INFINITY
                } else {
                    20.0 * mixed.log10()
                };
            }
        }
    }

    pub fn hold_levels(&self) -> [f32; 2] {
        self.hold_levels
    }
}

#[cfg(test)]
impl MeterState {
    pub(crate) fn set_last_update_for_test(&mut self, instant: Instant) {
        self.last_update = instant;
    }
}

pub fn paint_meter(painter: &Painter, rect: Rect, levels: &MeterState, theme: &MixerTheme) {
    let bg_rect = rect.expand2(Vec2::new(0.0, 0.0));
    painter.rect_filled(bg_rect, 2.0, theme.meter_bg);

    let left_rect = Rect::from_min_max(bg_rect.min, pos2(bg_rect.center().x - 2.0, bg_rect.max.y));
    let right_rect = Rect::from_min_max(pos2(bg_rect.center().x + 2.0, bg_rect.min.y), bg_rect.max);

    paint_channel_meter(
        painter,
        left_rect,
        levels.last_levels.left_peak,
        levels.hold_levels[0],
        theme,
    );
    paint_channel_meter(
        painter,
        right_rect,
        levels.last_levels.right_peak,
        levels.hold_levels[1],
        theme,
    );

    if levels.last_levels.clipped {
        let clip_rect = Rect::from_min_size(
            pos2(bg_rect.center().x - 8.0, bg_rect.min.y - 16.0),
            Vec2::new(16.0, 12.0),
        );
        painter.rect_filled(clip_rect, 4.0, theme.clip);
    }
}

fn paint_channel_meter(
    painter: &Painter,
    rect: Rect,
    peak_db: f32,
    hold_db: f32,
    theme: &MixerTheme,
) {
    let peak_linear = 10.0f32.powf(peak_db / 20.0).clamp(0.0, 1.0);
    let hold_linear = 10.0f32.powf(hold_db / 20.0).clamp(0.0, 1.0);

    let peak_height = rect.height() * peak_linear;
    let peak_rect = Rect::from_min_max(pos2(rect.min.x, rect.max.y - peak_height), rect.max);

    painter.rect_filled(peak_rect, 2.0, lerp_meter_color(peak_linear, theme));

    let hold_height = rect.height() * hold_linear;
    let hold_y = rect.max.y - hold_height;
    let hold_rect = Rect::from_min_max(
        pos2(rect.min.x, hold_y - 1.0),
        pos2(rect.max.x, hold_y + 1.0),
    );
    painter.rect_filled(hold_rect, 1.0, theme.meter_true_peak);
}

fn lerp_meter_color(t: f32, theme: &MixerTheme) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let r = theme.meter_grad_low.r() as f32
        + (theme.meter_grad_high.r() as f32 - theme.meter_grad_low.r() as f32) * t;
    let g = theme.meter_grad_low.g() as f32
        + (theme.meter_grad_high.g() as f32 - theme.meter_grad_low.g() as f32) * t;
    let b = theme.meter_grad_low.b() as f32
        + (theme.meter_grad_high.b() as f32 - theme.meter_grad_low.b() as f32) * t;
    Color32::from_rgb(r as u8, g as u8, b as u8)
}
