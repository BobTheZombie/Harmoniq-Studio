use crate::widget_framework::{
    MeterLevels, ScalarParameter, ToggleParameter, WidgetBinding, WidgetContext, WidgetControl,
    WidgetLayout, WidgetNode, WidgetSkin,
};
use crate::HarmoniqPalette;

/// Bundles mutable references to the grand piano clap parameters so the UI can
/// manipulate them directly.
pub struct GrandPianoClapParams<'a> {
    pub piano_level: &'a mut f32,
    pub clap_level: &'a mut f32,
    pub tone: &'a mut f32,
    pub sparkle: &'a mut f32,
    pub body: &'a mut f32,
    pub width: &'a mut f32,
    pub clap_delay: &'a mut f32,
    pub clap_tightness: &'a mut f32,
    pub attack: &'a mut f32,
    pub decay: &'a mut f32,
    pub sustain: &'a mut f32,
    pub release: &'a mut f32,
}

impl<'a> GrandPianoClapParams<'a> {
    pub fn new(
        piano_level: &'a mut f32,
        clap_level: &'a mut f32,
        tone: &'a mut f32,
        sparkle: &'a mut f32,
        body: &'a mut f32,
        width: &'a mut f32,
        clap_delay: &'a mut f32,
        clap_tightness: &'a mut f32,
        attack: &'a mut f32,
        decay: &'a mut f32,
        sustain: &'a mut f32,
        release: &'a mut f32,
    ) -> Self {
        Self {
            piano_level,
            clap_level,
            tone,
            sparkle,
            body,
            width,
            clap_delay,
            clap_tightness,
            attack,
            decay,
            sustain,
            release,
        }
    }
}

const PARAM_PIANO_LEVEL: &str = "piano_level";
const PARAM_CLAP_LEVEL: &str = "clap_level";
const PARAM_TONE: &str = "tone";
const PARAM_SPARKLE: &str = "sparkle";
const PARAM_BODY: &str = "body";
const PARAM_WIDTH: &str = "width";
const PARAM_CLAP_DELAY: &str = "clap_delay";
const PARAM_CLAP_TIGHTNESS: &str = "clap_tightness";
const PARAM_ATTACK: &str = "attack";
const PARAM_DECAY: &str = "decay";
const PARAM_SUSTAIN: &str = "sustain";
const PARAM_RELEASE: &str = "release";

impl<'a> WidgetContext for GrandPianoClapParams<'a> {
    fn bind_scalar(&mut self, binding: &WidgetBinding) -> Option<ScalarParameter<'_>> {
        match binding.parameter() {
            PARAM_PIANO_LEVEL => Some(ScalarParameter::new(&mut *self.piano_level)),
            PARAM_CLAP_LEVEL => Some(ScalarParameter::new(&mut *self.clap_level)),
            PARAM_TONE => Some(ScalarParameter::new(&mut *self.tone)),
            PARAM_SPARKLE => Some(ScalarParameter::new(&mut *self.sparkle)),
            PARAM_BODY => Some(ScalarParameter::new(&mut *self.body)),
            PARAM_WIDTH => Some(ScalarParameter::new(&mut *self.width)),
            PARAM_CLAP_DELAY => Some(ScalarParameter::new(&mut *self.clap_delay)),
            PARAM_CLAP_TIGHTNESS => Some(ScalarParameter::new(&mut *self.clap_tightness)),
            PARAM_ATTACK => Some(ScalarParameter::new(&mut *self.attack)),
            PARAM_DECAY => Some(ScalarParameter::new(&mut *self.decay)),
            PARAM_SUSTAIN => Some(ScalarParameter::new(&mut *self.sustain)),
            PARAM_RELEASE => Some(ScalarParameter::new(&mut *self.release)),
            _ => None,
        }
    }

    fn bind_toggle(&mut self, _binding: &WidgetBinding) -> Option<ToggleParameter<'_>> {
        None
    }

    fn meter_levels(&mut self, _binding: &WidgetBinding) -> Option<MeterLevels> {
        None
    }
}

/// Renders the custom UI for the Grand Piano Clap instrument, returning the
/// [`egui::Response`] from the surrounding group.
pub fn show_grand_piano_clap_ui(
    ui: &mut egui::Ui,
    params: GrandPianoClapParams<'_>,
    palette: &HarmoniqPalette,
) -> egui::Response {
    let mut params = params;
    let layout = grand_piano_layout();
    let skin = WidgetSkin::default()
        .with_knob_diameter(62.0)
        .with_fader_height(110.0)
        .with_toggle_width(52.0)
        .with_group_fill(palette.panel_alt);
    layout.render(ui, &mut params, palette, &skin)
}

fn grand_piano_layout() -> WidgetLayout {
    WidgetLayout::new(vec![
        WidgetNode::control(WidgetControl::heading("Grand Piano Clap")),
        WidgetNode::control(WidgetControl::spacer(6.0)),
        WidgetNode::row(vec![
            WidgetNode::control(WidgetControl::knob(
                "piano",
                PARAM_PIANO_LEVEL,
                0.0..=1.5,
                0.85,
                "Piano",
            )),
            WidgetNode::control(WidgetControl::knob(
                "clap",
                PARAM_CLAP_LEVEL,
                0.0..=1.5,
                0.65,
                "Clap",
            )),
            WidgetNode::control(WidgetControl::knob(
                "tone",
                PARAM_TONE,
                0.0..=1.0,
                0.55,
                "Tone",
            )),
            WidgetNode::control(WidgetControl::knob(
                "sparkle",
                PARAM_SPARKLE,
                0.0..=1.0,
                0.35,
                "Sparkle",
            )),
        ]),
        WidgetNode::control(WidgetControl::spacer(8.0)),
        WidgetNode::row(vec![
            WidgetNode::control(WidgetControl::knob(
                "body",
                PARAM_BODY,
                0.0..=1.0,
                0.35,
                "Body",
            )),
            WidgetNode::control(WidgetControl::knob(
                "width",
                PARAM_WIDTH,
                0.0..=1.0,
                0.65,
                "Width",
            )),
            WidgetNode::control(WidgetControl::knob(
                "delay",
                PARAM_CLAP_DELAY,
                0.0..=0.25,
                0.05,
                "Delay",
            )),
            WidgetNode::control(WidgetControl::knob(
                "tightness",
                PARAM_CLAP_TIGHTNESS,
                0.5..=1.5,
                1.0,
                "Tight",
            )),
        ]),
        WidgetNode::control(WidgetControl::spacer(10.0)),
        WidgetNode::group(
            Some("Envelope".to_string()),
            vec![WidgetNode::row(vec![
                WidgetNode::control(
                    WidgetControl::fader("attack", PARAM_ATTACK, 0.001..=0.2, 0.01)
                        .with_height(110.0)
                        .with_fader_label("Attack"),
                ),
                WidgetNode::control(
                    WidgetControl::fader("decay", PARAM_DECAY, 0.05..=2.0, 0.35)
                        .with_height(110.0)
                        .with_fader_label("Decay"),
                ),
                WidgetNode::control(
                    WidgetControl::fader("sustain", PARAM_SUSTAIN, 0.0..=1.0, 0.65)
                        .with_height(110.0)
                        .with_fader_label("Sustain"),
                ),
                WidgetNode::control(
                    WidgetControl::fader("release", PARAM_RELEASE, 0.05..=2.5, 0.45)
                        .with_height(110.0)
                        .with_fader_label("Release"),
                ),
            ])],
        ),
    ])
}
