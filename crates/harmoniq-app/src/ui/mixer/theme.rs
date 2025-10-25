use std::cell::RefCell;

use eframe::egui::Color32;
use harmoniq_ui::HarmoniqPalette;

#[derive(Clone, Debug)]
pub struct MixerTheme {
    pub panel_bg: Color32,
    pub strip_bg: Color32,
    pub strip_border: Color32,
    pub meter_bg: Color32,
    pub meter_fill: Color32,
    pub meter_clip: Color32,
    pub text_primary: Color32,
    pub overlay: Color32,
    pub master_bg: Color32,
}

impl Default for MixerTheme {
    fn default() -> Self {
        Self {
            panel_bg: Color32::from_rgb(30, 30, 38),
            strip_bg: Color32::from_rgb(45, 45, 58),
            strip_border: Color32::from_rgb(64, 64, 80),
            meter_bg: Color32::from_rgb(26, 26, 32),
            meter_fill: Color32::from_rgb(10, 200, 120),
            meter_clip: Color32::from_rgb(220, 64, 64),
            text_primary: Color32::from_rgb(220, 220, 230),
            overlay: Color32::from_rgba_unmultiplied(255, 255, 255, 30),
            master_bg: Color32::from_rgb(52, 52, 72),
        }
    }
}

impl MixerTheme {
    pub fn from_palette(palette: &HarmoniqPalette) -> Self {
        Self {
            panel_bg: palette.panel,
            strip_bg: palette.mixer_strip_bg,
            strip_border: palette.mixer_strip_border,
            meter_bg: mix_color(palette.panel, Color32::BLACK, 0.6),
            meter_fill: palette.meter_low,
            meter_clip: palette.meter_peak,
            text_primary: palette.text_primary,
            overlay: Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            master_bg: mix_color(palette.mixer_strip_bg, Color32::WHITE, 0.1),
        }
    }
}

fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv_t = 1.0 - t;
    let [ar, ag, ab, aa] = a.to_array();
    let [br, bg, bb, ba] = b.to_array();
    let blend = |ac: u8, bc: u8| (ac as f32 * inv_t + bc as f32 * t).round() as u8;
    Color32::from_rgba_premultiplied(blend(ar, br), blend(ag, bg), blend(ab, bb), blend(aa, ba))
}

thread_local! {
    static ACTIVE_THEME: RefCell<MixerTheme> = RefCell::new(MixerTheme::default());
}

pub fn with_active_theme<R>(theme: &MixerTheme, f: impl FnOnce() -> R) -> R {
    ACTIVE_THEME.with(|cell| {
        let previous = cell.replace(theme.clone());
        let result = f();
        cell.replace(previous);
        result
    })
}

pub fn active_theme() -> MixerTheme {
    ACTIVE_THEME.with(|cell| cell.borrow().clone())
}
