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
    pub clip: Color32,
    pub meter_bg: Color32,
    pub meter_grad_low: Color32,
    pub meter_grad_high: Color32,
    pub meter_true_peak: Color32,
    pub inactive_slot: Color32,
    pub active_slot: Color32,
    pub slot_border: Stroke,
    pub rounding_small: Rounding,
    pub rounding_large: Rounding,
}

impl MixerTheme {
    pub fn dark() -> Self {
        Self {
            background: Color32::from_rgb(14, 14, 18),
            strip_bg: Color32::from_rgb(22, 22, 28),
            strip_border: Stroke::new(1.0, Color32::from_rgb(38, 38, 48)),
            header_text: Color32::from_rgb(220, 220, 232),
            muted: Color32::from_rgb(132, 132, 144),
            soloed: Color32::from_rgb(245, 214, 123),
            armed: Color32::from_rgb(250, 95, 95),
            accent: Color32::from_rgb(138, 43, 226),
            clip: Color32::from_rgb(255, 62, 62),
            meter_bg: Color32::from_rgb(18, 18, 22),
            meter_grad_low: Color32::from_rgb(80, 160, 120),
            meter_grad_high: Color32::from_rgb(190, 230, 120),
            meter_true_peak: Color32::from_rgb(226, 120, 210),
            inactive_slot: Color32::from_rgb(40, 40, 48),
            active_slot: Color32::from_rgb(70, 70, 90),
            slot_border: Stroke::new(1.0, Color32::from_rgb(55, 55, 65)),
            rounding_small: Rounding::same(4.0),
            rounding_large: Rounding::same(10.0),
        }
    }
}
