use egui::{self, Align2, Color32, FontId, Response, Sense, Vec2};

use crate::theme::HarmoniqPalette;

pub struct Fader<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    default: f32,
    height: f32,
    palette: &'a HarmoniqPalette,
}

impl<'a> Fader<'a> {
    pub fn new(
        value: &'a mut f32,
        min: f32,
        max: f32,
        default: f32,
        palette: &'a HarmoniqPalette,
    ) -> Self {
        Self {
            value,
            min,
            max,
            default,
            height: 156.0,
            palette,
        }
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height.max(80.0);
        self
    }
}

impl<'a> egui::Widget for Fader<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let width = 32.0;
        let (rect, mut response) =
            ui.allocate_exact_size(egui::vec2(width, self.height), Sense::click_and_drag());
        let mut value = (*self.value).clamp(self.min, self.max);

        if response.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta().y);
            let sensitivity = (self.max - self.min).abs() / self.height.max(1.0);
            value -= delta * sensitivity;
            value = value.clamp(self.min, self.max);
            *self.value = value;
            response.mark_changed();
            ui.ctx().request_repaint();
        } else {
            *self.value = value;
        }

        if response.double_clicked() {
            *self.value = self.default.clamp(self.min, self.max);
            response.mark_changed();
        }

        let track_rect = rect.shrink2(egui::vec2(width * 0.3, 10.0));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, self.palette.meter_background);
        painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, self.palette.meter_border));

        let normalized = (value - self.min) / (self.max - self.min).max(1e-6);
        let handle_y = track_rect.bottom() - normalized * track_rect.height();
        let handle_rect = egui::Rect::from_center_size(
            egui::pos2(track_rect.center().x, handle_y),
            egui::vec2(track_rect.width() + 6.0, 14.0),
        );

        painter.rect_filled(track_rect, 4.0, self.palette.toolbar_highlight);
        painter.rect_filled(handle_rect, 6.0, self.palette.accent);
        painter.rect_stroke(
            handle_rect,
            6.0,
            egui::Stroke::new(1.0, self.palette.toolbar_outline),
        );

        response
    }
}

pub struct Knob<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    default: f32,
    label: &'a str,
    palette: &'a HarmoniqPalette,
    diameter: f32,
}

impl<'a> Knob<'a> {
    pub fn new(
        value: &'a mut f32,
        min: f32,
        max: f32,
        default: f32,
        label: &'a str,
        palette: &'a HarmoniqPalette,
    ) -> Self {
        Self {
            value,
            min,
            max,
            default,
            label,
            palette,
            diameter: 56.0,
        }
    }

    pub fn with_diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(28.0);
        self
    }
}

impl<'a> egui::Widget for Knob<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let knob_diameter = self.diameter;
        let label_height = 18.0;
        let desired_size = egui::vec2(knob_diameter + 16.0, knob_diameter + label_height + 12.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::drag());
        let mut value = (*self.value).clamp(self.min, self.max);

        if response.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta().y);
            let sensitivity = (self.max - self.min).abs() / 160.0;
            value -= delta * sensitivity;
            value = value.clamp(self.min, self.max);
            *self.value = value;
            response.mark_changed();
            ui.ctx().request_repaint();
        } else {
            *self.value = value;
        }

        if response.double_clicked() {
            *self.value = self.default.clamp(self.min, self.max);
            response.mark_changed();
        }

        let knob_radius = knob_diameter * 0.5;
        let knob_center = egui::pos2(rect.center().x, rect.top() + knob_radius + 6.0);
        let painter = ui.painter_at(rect);
        painter.circle_filled(knob_center, knob_radius, self.palette.knob_base);
        painter.circle_stroke(
            knob_center,
            knob_radius,
            egui::Stroke::new(2.0, self.palette.knob_ring),
        );

        let normalized = (value - self.min) / (self.max - self.min).max(1e-6);
        let angle = (-135.0_f32.to_radians()) + normalized * (270.0_f32.to_radians());
        let indicator = egui::pos2(
            knob_center.x + angle.cos() * (knob_radius - 6.0),
            knob_center.y + angle.sin() * (knob_radius - 6.0),
        );
        painter.line_segment(
            [knob_center, indicator],
            egui::Stroke::new(3.0, self.palette.knob_indicator),
        );
        painter.circle_filled(knob_center, 3.0, self.palette.knob_indicator);

        let label_pos = egui::pos2(rect.center().x, rect.bottom() - 6.0);
        painter.text(
            label_pos,
            Align2::CENTER_BOTTOM,
            self.label,
            FontId::proportional(12.0),
            self.palette.knob_label,
        );

        response
    }
}

pub struct LevelMeter<'a> {
    palette: &'a HarmoniqPalette,
    size: Vec2,
    left: f32,
    right: f32,
    rms: f32,
}

impl<'a> LevelMeter<'a> {
    pub fn new(palette: &'a HarmoniqPalette) -> Self {
        Self {
            palette,
            size: egui::vec2(18.0, 156.0),
            left: 0.0,
            right: 0.0,
            rms: 0.0,
        }
    }

    pub fn with_levels(mut self, left: f32, right: f32, rms: f32) -> Self {
        self.left = left;
        self.right = right;
        self.rms = rms;
        self
    }

    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }
}

impl<'a> egui::Widget for LevelMeter<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        let painter = ui.painter_at(rect);
        let mut mesh = egui::Mesh::default();
        let mut push_rect = |mesh: &mut egui::Mesh, rect: egui::Rect, color: Color32| {
            let idx = mesh.vertices.len() as u32;
            mesh.indices
                .extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: rect.left_top(),
                uv: egui::Pos2::ZERO,
                color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(rect.right(), rect.top()),
                uv: egui::Pos2::ZERO,
                color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: rect.right_bottom(),
                uv: egui::Pos2::ZERO,
                color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(rect.left(), rect.bottom()),
                uv: egui::Pos2::ZERO,
                color,
            });
        };
        push_rect(&mut mesh, rect, self.palette.meter_background);

        let gutter = 4.0;
        let bar_width = (rect.width() - gutter * 3.0) / 2.0;
        let max_height = rect.height() - gutter * 2.0;
        let segments = [
            (0.0, 0.55, self.palette.meter_low),
            (0.55, 0.8, self.palette.meter_mid),
            (0.8, 0.95, self.palette.meter_high),
            (0.95, 1.0, self.palette.meter_peak),
        ];

        let mut draw_channel = |level: f32, x_start: f32| {
            let level = level.clamp(0.0, 1.0);
            let x_end = x_start + bar_width;
            for &(start, end, color) in &segments {
                if level <= start {
                    continue;
                }
                let segment_end = level.min(end);
                if segment_end <= start {
                    continue;
                }
                let start_y = rect.bottom() - gutter - start * max_height;
                let end_y = rect.bottom() - gutter - segment_end * max_height;
                if end_y >= start_y {
                    continue;
                }
                let segment_rect = egui::Rect::from_min_max(
                    egui::pos2(x_start, end_y),
                    egui::pos2(x_end, start_y),
                );
                push_rect(&mut mesh, segment_rect, color);
            }
        };

        let left_start = rect.left() + gutter;
        let right_start = rect.left() + gutter * 2.0 + bar_width;
        draw_channel(self.left, left_start);
        draw_channel(self.right, right_start);

        painter.add(mesh);
        painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, self.palette.meter_border));

        let tick_color = self.palette.meter_border.gamma_multiply(0.6);
        for tick in [0.25_f32, 0.5, 0.75] {
            let y = rect.bottom() - gutter - tick * max_height;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + gutter * 0.6, y),
                    egui::pos2(rect.right() - gutter * 0.6, y),
                ],
                egui::Stroke::new(0.5, tick_color),
            );
        }

        let rms_height = self.rms.clamp(0.0, 1.0) * max_height;
        let rms_y = rect.bottom() - gutter - rms_height;
        painter.line_segment(
            [
                egui::pos2(rect.left() + gutter, rms_y),
                egui::pos2(rect.right() - gutter, rms_y),
            ],
            egui::Stroke::new(1.0, self.palette.meter_rms),
        );

        response
    }
}

pub struct StateToggleButton<'a> {
    value: &'a mut bool,
    label: &'a str,
    palette: &'a HarmoniqPalette,
    width: f32,
}

impl<'a> StateToggleButton<'a> {
    pub fn new(value: &'a mut bool, label: &'a str, palette: &'a HarmoniqPalette) -> Self {
        Self {
            value,
            label,
            palette,
            width: 32.0,
        }
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width.max(28.0);
        self
    }
}

impl<'a> egui::Widget for StateToggleButton<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let desired_size = egui::vec2(self.width, 28.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
        let mut value = *self.value;
        if response.clicked() {
            value = !value;
            *self.value = value;
            response.mark_changed();
        }

        let fill = if value {
            self.palette.mixer_toggle_active
        } else {
            self.palette.mixer_toggle_inactive
        };
        let text_color = if value {
            self.palette.text_primary
        } else {
            self.palette.mixer_toggle_text
        };

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, fill);
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, self.palette.mixer_strip_border),
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            self.label,
            FontId::proportional(13.0),
            text_color,
        );

        response
    }
}

pub struct StepToggle<'a> {
    palette: &'a HarmoniqPalette,
    accent: Color32,
    active: bool,
    emphasise: bool,
    size: Vec2,
}

impl<'a> StepToggle<'a> {
    pub fn new(palette: &'a HarmoniqPalette, accent: Color32) -> Self {
        Self {
            palette,
            accent,
            active: false,
            emphasise: false,
            size: egui::vec2(18.0, 32.0),
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn emphasise(mut self, emphasise: bool) -> Self {
        self.emphasise = emphasise;
        self
    }

    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }
}

impl<'a> egui::Widget for StepToggle<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let base = if self.emphasise {
            self.palette.toolbar_highlight
        } else {
            self.palette.panel
        };
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::click());
        let mut fill = if self.active {
            self.accent
        } else {
            base.gamma_multiply(1.05)
        };
        if response.hovered() {
            fill = fill.gamma_multiply(1.08);
        }
        let stroke = if self.active {
            self.palette.accent_alt.gamma_multiply(1.1)
        } else {
            self.palette.toolbar_outline
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect.shrink(2.0), 4.0, fill);
        painter.rect_stroke(rect.shrink(2.0), 4.0, egui::Stroke::new(1.0, stroke));
        response
    }
}

pub struct NoteBlock<'a> {
    rect: egui::Rect,
    palette: &'a HarmoniqPalette,
    base_color: Color32,
    border_color: Color32,
    selected: bool,
    rounding: f32,
}

impl<'a> NoteBlock<'a> {
    pub fn new(
        rect: egui::Rect,
        palette: &'a HarmoniqPalette,
        base_color: Color32,
        border_color: Color32,
    ) -> Self {
        Self {
            rect,
            palette,
            base_color,
            border_color,
            selected: false,
            rounding: 4.0,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_rounding(mut self, rounding: f32) -> Self {
        self.rounding = rounding.max(0.0);
        self
    }
}

impl<'a> egui::Widget for NoteBlock<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let rect = self.rect;
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let mut fill = self.base_color.gamma_multiply(0.95);
        if self.selected {
            fill = fill.gamma_multiply(1.25);
        } else if response.hovered() {
            fill = fill.gamma_multiply(1.1);
        }
        let border = if self.selected {
            self.palette.clip_border_active
        } else {
            self.border_color
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, self.rounding, fill);
        painter.rect_stroke(rect, self.rounding, egui::Stroke::new(1.0, border));
        response
    }
}
