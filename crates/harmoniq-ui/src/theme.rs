use egui::epaint::Margin;
use egui::{self, Color32, Context, FontId, Rounding, Stroke, TextStyle};

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
            background: Color32::from_rgb(18, 20, 24),
            panel: Color32::from_rgb(24, 27, 33),
            panel_alt: Color32::from_rgb(30, 34, 41),
            toolbar: Color32::from_rgb(27, 31, 39),
            toolbar_highlight: Color32::from_rgb(40, 45, 56),
            toolbar_outline: Color32::from_rgb(60, 68, 82),
            text_primary: Color32::from_rgb(240, 243, 248),
            text_muted: Color32::from_rgb(152, 161, 179),
            accent: Color32::from_rgb(61, 155, 243),
            accent_alt: Color32::from_rgb(86, 181, 255),
            accent_soft: Color32::from_rgb(42, 108, 201),
            success: Color32::from_rgb(62, 201, 141),
            warning: Color32::from_rgb(255, 123, 110),
            track_header: Color32::from_rgb(33, 38, 46),
            track_header_selected: Color32::from_rgb(44, 60, 86),
            track_lane_overlay: Color32::from_rgba_unmultiplied(61, 155, 243, 36),
            track_button_bg: Color32::from_rgb(36, 41, 50),
            track_button_border: Color32::from_rgb(28, 32, 39),
            automation_header: Color32::from_rgb(37, 43, 52),
            automation_header_muted: Color32::from_rgb(31, 35, 42),
            automation_lane_bg: Color32::from_rgb(27, 31, 38),
            automation_lane_hidden_bg: Color32::from_rgb(22, 25, 31),
            automation_point_border: Color32::from_rgb(50, 58, 70),
            timeline_bg: Color32::from_rgb(26, 30, 37),
            timeline_header: Color32::from_rgb(33, 38, 46),
            timeline_border: Color32::from_rgb(70, 78, 92),
            timeline_grid_primary: Color32::from_rgb(52, 86, 132),
            timeline_grid_secondary: Color32::from_rgb(46, 52, 63),
            ruler_text: Color32::from_rgb(204, 210, 224),
            clip_text_primary: Color32::from_rgb(234, 237, 244),
            clip_text_secondary: Color32::from_rgb(184, 192, 206),
            clip_border_default: Color32::from_rgb(60, 66, 78),
            clip_border_active: Color32::from_rgb(61, 155, 243),
            clip_border_playing: Color32::from_rgb(107, 191, 255),
            clip_shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 120),
            piano_background: Color32::from_rgb(21, 24, 30),
            piano_grid_major: Color32::from_rgb(58, 66, 78),
            piano_grid_minor: Color32::from_rgb(44, 50, 60),
            piano_white: Color32::from_rgb(242, 244, 248),
            piano_black: Color32::from_rgb(70, 75, 86),
            meter_background: Color32::from_rgb(28, 30, 36),
            meter_border: Color32::from_rgb(52, 60, 72),
            meter_low: Color32::from_rgb(74, 206, 162),
            meter_mid: Color32::from_rgb(244, 196, 96),
            meter_high: Color32::from_rgb(243, 147, 96),
            meter_peak: Color32::from_rgb(240, 102, 109),
            meter_rms: Color32::from_rgb(122, 196, 255),
            knob_base: Color32::from_rgb(46, 52, 63),
            knob_ring: Color32::from_rgb(61, 155, 243),
            knob_indicator: Color32::from_rgb(141, 197, 255),
            knob_label: Color32::from_rgb(214, 220, 232),
            mixer_strip_bg: Color32::from_rgb(27, 32, 40),
            mixer_strip_selected: Color32::from_rgb(36, 54, 76),
            mixer_strip_solo: Color32::from_rgb(24, 60, 64),
            mixer_strip_muted: Color32::from_rgb(56, 44, 52),
            mixer_strip_border: Color32::from_rgb(58, 74, 98),
            mixer_strip_header: Color32::from_rgb(30, 38, 52),
            mixer_strip_header_selected: Color32::from_rgb(54, 86, 124),
            mixer_slot_bg: Color32::from_rgb(28, 34, 46),
            mixer_slot_active: Color32::from_rgb(42, 62, 88),
            mixer_slot_border: Color32::from_rgb(70, 86, 112),
            mixer_toggle_active: Color32::from_rgb(112, 187, 255),
            mixer_toggle_inactive: Color32::from_rgb(30, 36, 46),
            mixer_toggle_text: Color32::from_rgb(226, 236, 248),
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

    pub fn palette_mut(&mut self) -> &mut HarmoniqPalette {
        &mut self.palette
    }
}
