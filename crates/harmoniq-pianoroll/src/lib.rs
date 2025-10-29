pub mod state;
pub mod ui;

use state::{MidiClip, PianoRollState};

/// Props for rendering the piano roll.
pub struct PianoRollProps<'a> {
    pub state: &'a mut PianoRollState,
    pub snap: ui::Snap,
    pub ghost_clip: Option<&'a MidiClip>,
    pub scale: ui::ScaleGuide,
    /// Called whenever the clip's notes have changed (UI thread; non-RT).
    pub on_changed: Box<dyn FnMut(&MidiClip) + 'a>,
    /// Optional callback to preview notes (key, vel) when drawing; non-RT.
    pub on_preview: Option<Box<dyn FnMut(i8, u8) + 'a>>,
}

pub fn render(ui: &mut egui::Ui, props: PianoRollProps) {
    ui::pianoroll::render(ui, props);
}
