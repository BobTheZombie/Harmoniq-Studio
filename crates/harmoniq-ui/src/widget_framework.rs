use std::ops::RangeInclusive;

use egui::{self, Align, Color32, Frame, Margin, Rounding};

use crate::{theme::HarmoniqPalette, Fader, Knob, LevelMeter, StateToggleButton};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WidgetId(String);

impl WidgetId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct WidgetBinding {
    parameter: String,
}

impl WidgetBinding {
    pub fn new(parameter: impl Into<String>) -> Self {
        Self {
            parameter: parameter.into(),
        }
    }

    pub fn parameter(&self) -> &str {
        &self.parameter
    }
}

pub trait WidgetContext {
    fn bind_scalar(&mut self, binding: &WidgetBinding) -> Option<ScalarParameter<'_>>;
    fn bind_toggle(&mut self, binding: &WidgetBinding) -> Option<ToggleParameter<'_>>;
    fn meter_levels(&mut self, binding: &WidgetBinding) -> Option<MeterLevels>;
}

pub struct ScalarParameter<'a> {
    value: &'a mut f32,
}

impl<'a> ScalarParameter<'a> {
    pub fn new(value: &'a mut f32) -> Self {
        Self { value }
    }

    pub fn value(&mut self) -> &mut f32 {
        self.value
    }
}

pub struct ToggleParameter<'a> {
    value: &'a mut bool,
}

impl<'a> ToggleParameter<'a> {
    pub fn new(value: &'a mut bool) -> Self {
        Self { value }
    }

    pub fn value(&mut self) -> &mut bool {
        self.value
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MeterLevels {
    pub left: f32,
    pub right: f32,
    pub rms: f32,
}

#[derive(Debug, Clone)]
pub enum WidgetKind {
    Knob {
        range: RangeInclusive<f32>,
        default: f32,
        label: String,
    },
    Fader {
        range: RangeInclusive<f32>,
        default: f32,
        height: Option<f32>,
        label: Option<String>,
    },
    Toggle {
        label: String,
        width: Option<f32>,
    },
    LevelMeter {
        width: f32,
        height: f32,
    },
    Label(String),
    Heading(String),
    Spacer(f32),
}

#[derive(Debug, Clone)]
pub struct WidgetControl {
    pub id: WidgetId,
    pub binding: Option<WidgetBinding>,
    pub kind: WidgetKind,
}

impl WidgetControl {
    pub fn knob(
        id: impl Into<String>,
        binding: impl Into<String>,
        range: RangeInclusive<f32>,
        default: f32,
        label: impl Into<String>,
    ) -> Self {
        let label = label.into();
        Self {
            id: WidgetId::new(id),
            binding: Some(WidgetBinding::new(binding)),
            kind: WidgetKind::Knob {
                range,
                default,
                label,
            },
        }
    }

    pub fn fader(
        id: impl Into<String>,
        binding: impl Into<String>,
        range: RangeInclusive<f32>,
        default: f32,
    ) -> Self {
        Self {
            id: WidgetId::new(id),
            binding: Some(WidgetBinding::new(binding)),
            kind: WidgetKind::Fader {
                range,
                default,
                height: None,
                label: None,
            },
        }
    }

    pub fn toggle(
        id: impl Into<String>,
        binding: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        let label = label.into();
        Self {
            id: WidgetId::new(id),
            binding: Some(WidgetBinding::new(binding)),
            kind: WidgetKind::Toggle { label, width: None },
        }
    }

    pub fn level_meter(
        id: impl Into<String>,
        binding: impl Into<String>,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            id: WidgetId::new(id),
            binding: Some(WidgetBinding::new(binding)),
            kind: WidgetKind::LevelMeter { width, height },
        }
    }

    pub fn heading(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id: WidgetId::new(format!("heading-{}", text)),
            binding: None,
            kind: WidgetKind::Heading(text),
        }
    }

    pub fn label(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id: WidgetId::new(format!("label-{}", text)),
            binding: None,
            kind: WidgetKind::Label(text),
        }
    }

    pub fn spacer(amount: f32) -> Self {
        Self {
            id: WidgetId::new(format!("spacer-{amount}")),
            binding: None,
            kind: WidgetKind::Spacer(amount),
        }
    }

    pub fn with_height(mut self, height: f32) -> Self {
        if let WidgetKind::Fader { height: slot, .. } = &mut self.kind {
            *slot = Some(height);
        }
        self
    }

    pub fn with_toggle_width(mut self, width: f32) -> Self {
        if let WidgetKind::Toggle { width: slot, .. } = &mut self.kind {
            *slot = Some(width);
        }
        self
    }

    pub fn with_fader_label(mut self, label: impl Into<String>) -> Self {
        if let WidgetKind::Fader { label: slot, .. } = &mut self.kind {
            *slot = Some(label.into());
        }
        self
    }
}

#[derive(Debug, Clone)]
pub enum WidgetNode {
    Row(Vec<WidgetNode>),
    Column(Vec<WidgetNode>),
    Group {
        title: Option<String>,
        children: Vec<WidgetNode>,
    },
    Control(WidgetControl),
}

impl WidgetNode {
    pub fn row(children: Vec<WidgetNode>) -> Self {
        Self::Row(children)
    }

    pub fn column(children: Vec<WidgetNode>) -> Self {
        Self::Column(children)
    }

    pub fn group(title: Option<String>, children: Vec<WidgetNode>) -> Self {
        Self::Group { title, children }
    }

    pub fn control(control: WidgetControl) -> Self {
        Self::Control(control)
    }
}

#[derive(Debug, Clone)]
pub struct WidgetSkin {
    pub section_spacing: f32,
    pub row_spacing: f32,
    pub knob_diameter: f32,
    pub fader_height: f32,
    pub toggle_width: f32,
    pub group_rounding: f32,
    pub group_fill: Option<Color32>,
}

impl Default for WidgetSkin {
    fn default() -> Self {
        Self {
            section_spacing: 8.0,
            row_spacing: 6.0,
            knob_diameter: 56.0,
            fader_height: 120.0,
            toggle_width: 48.0,
            group_rounding: 10.0,
            group_fill: None,
        }
    }
}

impl WidgetSkin {
    pub fn with_knob_diameter(mut self, diameter: f32) -> Self {
        self.knob_diameter = diameter.max(28.0);
        self
    }

    pub fn with_fader_height(mut self, height: f32) -> Self {
        self.fader_height = height.max(64.0);
        self
    }

    pub fn with_toggle_width(mut self, width: f32) -> Self {
        self.toggle_width = width.max(32.0);
        self
    }

    pub fn with_group_fill(mut self, color: Color32) -> Self {
        self.group_fill = Some(color);
        self
    }
}

#[derive(Debug, Clone)]
pub struct WidgetLayout {
    pub nodes: Vec<WidgetNode>,
}

impl WidgetLayout {
    pub fn new(nodes: Vec<WidgetNode>) -> Self {
        Self { nodes }
    }

    pub fn render(
        &self,
        ui: &mut egui::Ui,
        ctx: &mut dyn WidgetContext,
        palette: &HarmoniqPalette,
        skin: &WidgetSkin,
    ) -> egui::Response {
        let mut frame = Frame::group(ui.style())
            .rounding(Rounding::same(skin.group_rounding))
            .inner_margin(Margin::symmetric(12.0, 10.0));
        if let Some(fill) = skin.group_fill {
            frame = frame.fill(fill);
        }
        frame
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(skin.row_spacing, skin.row_spacing);
                for node in &self.nodes {
                    render_node(ui, node, ctx, palette, skin);
                }
            })
            .response
    }
}

fn render_node(
    ui: &mut egui::Ui,
    node: &WidgetNode,
    ctx: &mut dyn WidgetContext,
    palette: &HarmoniqPalette,
    skin: &WidgetSkin,
) -> Option<egui::Response> {
    match node {
        WidgetNode::Row(children) => {
            let mut last = None;
            let inner = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(skin.row_spacing, skin.row_spacing);
                for child in children {
                    if let Some(response) = render_node(ui, child, ctx, palette, skin) {
                        last = Some(response);
                    }
                }
            });
            last.or(Some(inner.response))
        }
        WidgetNode::Column(children) => {
            let mut last = None;
            let inner = ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(skin.row_spacing, skin.row_spacing);
                for child in children {
                    if let Some(response) = render_node(ui, child, ctx, palette, skin) {
                        last = Some(response);
                    }
                }
            });
            last.or(Some(inner.response))
        }
        WidgetNode::Group { title, children } => {
            let mut frame = Frame::group(ui.style()).rounding(Rounding::same(skin.group_rounding));
            if let Some(fill) = skin.group_fill {
                frame = frame.fill(fill);
            }
            let response = frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(skin.row_spacing, skin.row_spacing);
                if let Some(title) = title {
                    ui.vertical_centered(|ui| {
                        ui.heading(title);
                    });
                    ui.add_space(skin.section_spacing);
                }
                for child in children {
                    let _ = render_node(ui, child, ctx, palette, skin);
                }
            });
            Some(response.response)
        }
        WidgetNode::Control(control) => render_control(ui, control, ctx, palette, skin),
    }
}

fn render_control(
    ui: &mut egui::Ui,
    control: &WidgetControl,
    ctx: &mut dyn WidgetContext,
    palette: &HarmoniqPalette,
    skin: &WidgetSkin,
) -> Option<egui::Response> {
    match &control.kind {
        WidgetKind::Knob {
            range,
            default,
            label,
        } => {
            let mut binding = ctx.bind_scalar(control.binding.as_ref()?);
            binding.as_mut().map(|binding| {
                ui.add(
                    Knob::new(
                        binding.value(),
                        *range.start(),
                        *range.end(),
                        *default,
                        label,
                        palette,
                    )
                    .with_diameter(skin.knob_diameter),
                )
            })
        }
        WidgetKind::Fader {
            range,
            default,
            height,
            label,
        } => {
            let mut binding = ctx.bind_scalar(control.binding.as_ref()?);
            binding.as_mut().map(|binding| {
                let mut response = None;
                ui.vertical(|ui| {
                    let fader = Fader::new(
                        binding.value(),
                        *range.start(),
                        *range.end(),
                        *default,
                        palette,
                    )
                    .with_height(height.unwrap_or(skin.fader_height));
                    response = Some(ui.add(fader));
                    if let Some(label) = label {
                        ui.add_space(4.0);
                        ui.with_layout(egui::Layout::top_down(Align::Center), |ui| {
                            ui.label(label);
                        });
                    }
                });
                response.expect("fader response should exist")
            })
        }
        WidgetKind::Toggle { label, width } => {
            let mut binding = ctx.bind_toggle(control.binding.as_ref()?);
            binding.as_mut().map(|binding| {
                ui.add(
                    StateToggleButton::new(binding.value(), label, palette)
                        .with_width(width.unwrap_or(skin.toggle_width)),
                )
            })
        }
        WidgetKind::LevelMeter { width, height } => {
            let levels = ctx.meter_levels(control.binding.as_ref()?);
            levels.map(|levels| {
                ui.add(
                    LevelMeter::new(palette)
                        .with_levels(levels.left, levels.right, levels.rms)
                        .with_size(egui::vec2(*width, *height)),
                )
            })
        }
        WidgetKind::Label(text) => Some(ui.label(text)),
        WidgetKind::Heading(text) => Some(ui.heading(text)),
        WidgetKind::Spacer(amount) => {
            ui.add_space(*amount);
            None
        }
    }
}
