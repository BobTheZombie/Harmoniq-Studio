use eframe::egui::{Color32, Rounding, Stroke};

/// Theme tokens used by the mixer widgets.
#[derive(Debug, Clone)]
pub struct MixerTheme {
    pub background: Color32,
    pub strip_bg: Color32,
    pub strip_border: Stroke,
    pub header_text: Color32,
    pub muted: Color32,
    pub soloed: Color32,
    pub armed: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub clip: Color32,
    pub meter_bg: Color32,
    pub meter_green: Color32,
    pub meter_yellow: Color32,
    pub meter_red: Color32,
    pub meter_true_peak: Color32,
    pub inactive_slot: Color32,
    pub active_slot: Color32,
    pub slot_border: Stroke,
    pub rounding_small: Rounding,
    pub rounding_large: Rounding,
    pub cap_gradient_top: Color32,
    pub cap_gradient_bottom: Color32,
    pub icon_bg: Color32,
    pub knob_bg: Color32,
    pub fader_track: Color32,
    pub fader_thumb: Color32,
    pub scale_tick: Color32,
    pub scale_text: Color32,
}

impl MixerTheme {
    pub fn dark() -> Self {
        Self {
            background: Color32::from_rgb(14, 14, 16),
            strip_bg: Color32::from_rgb(26, 26, 30),
            strip_border: Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 36)),
            header_text: Color32::from_rgb(212, 212, 224),
            muted: Color32::from_rgb(120, 120, 132),
            soloed: Color32::from_rgb(245, 214, 123),
            armed: Color32::from_rgb(250, 95, 95),
            accent: Color32::from_rgb(138, 43, 226),
            selection: Color32::from_rgb(138, 43, 226),
            clip: Color32::from_rgb(255, 62, 62),
            meter_bg: Color32::from_rgb(18, 18, 22),
            meter_green: Color32::from_rgb(58, 213, 106),
            meter_yellow: Color32::from_rgb(242, 233, 78),
            meter_red: Color32::from_rgb(255, 59, 48),
            meter_true_peak: Color32::from_rgb(226, 120, 210),
            inactive_slot: Color32::from_rgb(36, 36, 44),
            active_slot: Color32::from_rgb(68, 68, 92),
            slot_border: Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 28)),
            rounding_small: Rounding::same(4.0),
            rounding_large: Rounding::same(10.0),
            cap_gradient_top: Color32::from_rgb(32, 32, 40),
            cap_gradient_bottom: Color32::from_rgb(18, 18, 24),
            icon_bg: Color32::from_rgb(36, 36, 46),
            knob_bg: Color32::from_rgb(28, 28, 34),
            fader_track: Color32::from_rgb(28, 28, 34),
            fader_thumb: Color32::from_rgb(98, 98, 120),
            scale_tick: Color32::from_rgb(128, 128, 140),
            scale_text: Color32::from_rgb(160, 160, 172),
        }
    }
}
