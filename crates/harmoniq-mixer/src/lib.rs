pub mod state;

mod rt;
pub use rt::{
    AutoRx, AutoTx, AutomationEvent, AuxBusId, Command, CommandRx, CommandTx, Group, GroupId,
    Mixer, MixerConfig, RoutingBuilder, RoutingTable, TrackId,
};

#[cfg(feature = "egui")]
pub mod ui;

#[cfg(feature = "egui")]
use harmoniq_ui::HarmoniqPalette;
#[cfg(feature = "egui")]
use state::MixerState;
use state::{ChannelId, SendId};

/// Callbacks provided by the host app (non-RT).
pub struct MixerCallbacks {
    /// Open plugin browser to fill an insert slot (or append if slot is None)
    pub open_insert_browser: Box<dyn FnMut(ChannelId, Option<usize>) + Send>,
    /// Open a plugin's editor UI for the given insert slot
    pub open_insert_ui: Box<dyn FnMut(ChannelId, usize) + Send>,
    /// Toggle bypass on an insert slot (host should apply to engine)
    pub set_insert_bypass: Box<dyn FnMut(ChannelId, usize, bool) + Send>,
    /// Remove an insert slot (host should disconnect/remove from engine)
    pub remove_insert: Box<dyn FnMut(ChannelId, usize) + Send>,
    /// Reorder an insert slot (drag & drop)
    pub reorder_insert: Box<dyn FnMut(ChannelId, usize, usize) + Send>,
    /// Apply routing matrix changes
    pub apply_routing: Box<dyn FnMut(RoutingDelta) + Send>,
    /// Create/route a send target (A/B/C…) — host decides exact routing object
    pub configure_send: Box<dyn FnMut(ChannelId, SendId, f32) + Send>,
    /// Set channel gain (dB) and pan (-1..1) in engine
    pub set_gain_pan: Box<dyn FnMut(ChannelId, f32, f32) + Send>,
    /// Mute/Solo changes
    pub set_mute: Box<dyn FnMut(ChannelId, bool) + Send>,
    pub set_solo: Box<dyn FnMut(ChannelId, bool) + Send>,
}

impl MixerCallbacks {
    pub fn noop() -> Self {
        Self {
            open_insert_browser: Box::new(|_, _| {}),
            open_insert_ui: Box::new(|_, _| {}),
            set_insert_bypass: Box::new(|_, _, _| {}),
            remove_insert: Box::new(|_, _| {}),
            reorder_insert: Box::new(|_, _, _| {}),
            apply_routing: Box::new(|_| {}),
            configure_send: Box::new(|_, _, _| {}),
            set_gain_pan: Box::new(|_, _, _| {}),
            set_mute: Box::new(|_, _| {}),
            set_solo: Box::new(|_, _| {}),
        }
    }
}

#[cfg(feature = "egui")]
pub struct MixerProps<'a> {
    pub state: &'a mut MixerState,
    pub callbacks: &'a mut MixerCallbacks,
    pub palette: &'a HarmoniqPalette,
}

pub use state::{RoutingDelta, RoutingMatrix};

/// Render the mixer as a horizontal strip layout.
#[cfg(feature = "egui")]
pub fn render(ui: &mut egui::Ui, props: MixerProps) {
    ui::mixer::render(ui, props);
}

/// Simple helper to pan a mono sample into stereo (constant-power).
#[inline]
pub fn pan_mono(sample: f32, pan: f32) -> (f32, f32) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5; // [0..1]
    let angle = core::f32::consts::FRAC_PI_2 * p;
    let l = angle.cos();
    let r = angle.sin();
    (sample * l, sample * r)
}
