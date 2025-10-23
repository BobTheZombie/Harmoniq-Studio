use egui::{self, Color32, Context, FontId, Rounding, Stroke, TextStyle};
use egui::epaint::Margin;

#[derive(Clone)]
pub struct HarmoniqPalette {
    pub background: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub toolbar: Color32,
    pub toolbar_highlight: Color32,
    pub toolbar_outline: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_alt: Color32,
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub track_header: Color32,
    pub track_header_selected: Color32,
    pub track_lane_overlay: Color32,
    pub track_button_bg: Color32,
    pub track_button_border: Color32,
    pub automation_header: Color32,
    pub automation_header_muted: Color32,
    pub automation_lane_bg: Color32,
    pub automation_lane_hidden_bg: Color32,
    pub automation_point_border: Color32,
    pub timeline_bg: Color32,
    pub timeline_header: Color32,
    pub timeline_border: Color32,
    pub timeline_grid_primary: Color32,
    pub timeline_grid_secondary: Color32,
    pub ruler_text: Color32,
    pub clip_text_primary: Color32,
    pub clip_text_secondary: Color32,
    pub clip_border_default: Color32,
    pub clip_border_active: Color32,
    pub clip_border_playing: Color32,
    pub clip_shadow: Color32,
    pub piano_background: Color32,
    pub piano_grid_major: Color32,
    pub piano_grid_minor: Color32,
    pub piano_white: Color32,
    pub piano_black: Color32,
    pub meter_background: Color32,
    pub meter_border: Color32,
    pub meter_low: Color32,
    pub meter_mid: Color32,
    pub meter_high: Color32,
    pub meter_peak: Color32,
    pub meter_rms: Color32,
    pub knob_base: Color32,
    pub knob_ring: Color32,
    pub knob_indicator: Color32,
    pub knob_label: Color32,
    pub mixer_strip_bg: Color32,
    pub mixer_strip_selected: Color32,
    pub mixer_strip_solo: Color32,
    pub mixer_strip_muted: Color32,
    pub mixer_strip_border: Color32,
    pub mixer_strip_header: Color32,
    pub mixer_strip_header_selected: Color32,
    pub mixer_slot_bg: Color32,
    pub mixer_slot_active: Color32,
    pub mixer_slot_border: Color32,
    pub mixer_toggle_active: Color32,
    pub mixer_toggle_inactive: Color32,
    pub mixer_toggle_text: Color32,
}

impl HarmoniqPalette {
    pub fn new() -> Self {
        Self {
            background: Color32::from_rgb(30, 30, 30),
            panel: Color32::from_rgb(34, 34, 38),
            panel_alt: Color32::from_rgb(42, 42, 48),
            toolbar: Color32::from_rgb(36, 36, 42),
            toolbar_highlight: Color32::from_rgb(52, 52, 60),
            toolbar_outline: Color32::from_rgb(88, 88, 98),
            text_primary: Color32::from_rgb(232, 232, 240),
            text_muted: Color32::from_rgb(164, 164, 176),
            accent: Color32::from_rgb(138, 43, 226),
            accent_alt: Color32::from_rgb(166, 104, 239),
            accent_soft: Color32::from_rgb(112, 72, 196),
            success: Color32::from_rgb(82, 212, 164),
            warning: Color32::from_rgb(255, 120, 130),
            track_header: Color32::from_rgb(44, 44, 52),
            track_header_selected: Color32::from_rgb(59, 47, 79),
            track_lane_overlay: Color32::from_rgba_unmultiplied(138, 43, 226, 42),
            track_button_bg: Color32::from_rgb(48, 48, 56),
            track_button_border: Color32::from_rgb(32, 32, 38),
            automation_header: Color32::from_rgb(46, 46, 54),
            automation_header_muted: Color32::from_rgb(38, 38, 44),
            automation_lane_bg: Color32::from_rgb(34, 34, 40),
            automation_lane_hidden_bg: Color32::from_rgb(30, 30, 36),
            automation_point_border: Color32::from_rgb(54, 54, 64),
            timeline_bg: Color32::from_rgb(32, 32, 38),
            timeline_header: Color32::from_rgb(40, 40, 48),
            timeline_border: Color32::from_rgb(90, 90, 102),
            timeline_grid_primary: Color32::from_rgb(94, 80, 126),
            timeline_grid_secondary: Color32::from_rgb(58, 58, 72),
            ruler_text: Color32::from_rgb(204, 204, 214),
            clip_text_primary: Color32::from_rgb(236, 236, 246),
            clip_text_secondary: Color32::from_rgb(190, 190, 204),
            clip_border_default: Color32::from_rgb(68, 68, 80),
            clip_border_active: Color32::from_rgb(138, 43, 226),
            clip_border_playing: Color32::from_rgb(200, 140, 255),
            clip_shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 120),
            piano_background: Color32::from_rgb(28, 28, 32),
            piano_grid_major: Color32::from_rgb(70, 70, 82),
            piano_grid_minor: Color32::from_rgb(50, 50, 60),
            piano_white: Color32::from_rgb(242, 242, 248),
            piano_black: Color32::from_rgb(64, 64, 74),
            meter_background: Color32::from_rgb(36, 36, 42),
            meter_border: Color32::from_rgb(64, 64, 76),
            meter_low: Color32::from_rgb(94, 210, 170),
            meter_mid: Color32::from_rgb(255, 200, 132),
            meter_high: Color32::from_rgb(255, 150, 132),
            meter_peak: Color32::from_rgb(255, 98, 118),
            meter_rms: Color32::from_rgb(194, 166, 255),
            knob_base: Color32::from_rgb(52, 52, 64),
            knob_ring: Color32::from_rgb(138, 43, 226),
            knob_indicator: Color32::from_rgb(166, 104, 239),
            knob_label: Color32::from_rgb(210, 210, 220),
            mixer_strip_bg: Color32::from_rgb(40, 40, 48),
            mixer_strip_selected: Color32::from_rgb(62, 50, 80),
            mixer_strip_solo: Color32::from_rgb(56, 88, 78),
            mixer_strip_muted: Color32::from_rgb(86, 54, 68),
            mixer_strip_border: Color32::from_rgb(90, 90, 104),
            mixer_strip_header: Color32::from_rgb(48, 48, 58),
            mixer_strip_header_selected: Color32::from_rgb(68, 56, 92),
            mixer_slot_bg: Color32::from_rgb(36, 36, 44),
            mixer_slot_active: Color32::from_rgb(52, 44, 70),
            mixer_slot_border: Color32::from_rgb(82, 82, 96),
            mixer_toggle_active: Color32::from_rgb(138, 43, 226),
            mixer_toggle_inactive: Color32::from_rgb(40, 40, 48),
            mixer_toggle_text: Color32::from_rgb(232, 232, 240),
        }
    }
}

#[derive(Clone)]
pub struct HarmoniqTheme {
    palette: HarmoniqPalette,
}

impl HarmoniqTheme {
    pub fn init(ctx: &Context) -> Self {
        let palette = HarmoniqPalette::new();
        let mut style = (*ctx.style()).clone();
        let mut visuals = style.visuals.clone();
        visuals.dark_mode = true;
        visuals.override_text_color = Some(palette.text_primary);
        visuals.panel_fill = palette.background;
        visuals.window_fill = palette.panel;
        visuals.window_stroke = Stroke::new(1.0, palette.toolbar_outline);
        visuals.window_rounding = Rounding::same(10.0);
        visuals.widgets.noninteractive.bg_fill = palette.panel;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_muted);
        visuals.widgets.inactive.bg_fill = palette.panel_alt;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text_primary);
        visuals.widgets.inactive.rounding = Rounding::same(6.0);
        visuals.widgets.hovered.bg_fill = palette.accent_alt.gamma_multiply(0.6);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text_primary);
        visuals.widgets.hovered.rounding = Rounding::same(6.0);
        visuals.widgets.active.bg_fill = palette.accent.gamma_multiply(0.85);
        visuals.widgets.active.fg_stroke = Stroke::new(1.2, palette.text_primary);
        visuals.widgets.active.rounding = Rounding::same(6.0);
        visuals.widgets.open.bg_fill = palette.toolbar_highlight;
        visuals.selection.bg_fill = palette.accent_soft.gamma_multiply(0.85);
        visuals.selection.stroke = Stroke::new(1.0, palette.accent_alt);
        visuals.menu_rounding = Rounding::same(8.0);
        visuals.hyperlink_color = palette.accent_alt;
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.window_margin = Margin::same(10.0);
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(26.0)),
            (TextStyle::Body, FontId::proportional(17.0)),
            (TextStyle::Button, FontId::proportional(16.0)),
            (TextStyle::Small, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(15.0)),
        ]
        .into();
        ctx.set_style(style);
        Self { palette }
    }

    pub fn palette(&self) -> &HarmoniqPalette {
        &self.palette
    }
}
