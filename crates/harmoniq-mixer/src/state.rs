use std::collections::{HashMap, VecDeque};
use std::time::Instant;

pub type ChannelId = u32;
pub type InsertSlotId = usize;
pub type SendId = u8; // 0='A',1='B',...

pub const MAX_INSERT_SLOTS: usize = 10;
pub const MAX_SEND_SLOTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginFormat {
    Vst3,
    Clap,
}

impl PluginFormat {
    pub fn label(&self) -> &'static str {
        match self {
            PluginFormat::Vst3 => "VST3",
            PluginFormat::Clap => "CLAP",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanLaw {
    Linear,
    ConstantPower,
    Minus3Db,
}

impl Default for PanLaw {
    fn default() -> Self {
        Self::ConstantPower
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertPosition {
    PreFader,
    PostFader,
}

impl Default for InsertPosition {
    fn default() -> Self {
        Self::PreFader
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerViewTab {
    MixConsole,
    ChannelStrip,
    Meter,
    ControlRoom,
}

impl Default for MixerViewTab {
    fn default() -> Self {
        Self::MixConsole
    }
}

#[derive(Clone, Debug)]
pub struct MixerLayout {
    pub show_left_zone: bool,
    pub show_right_zone: bool,
    pub show_meter_bridge: bool,
    pub show_channel_racks: bool,
    pub show_history: bool,
    pub show_control_room: bool,
    pub strip_width: f32,
}

impl Default for MixerLayout {
    fn default() -> Self {
        Self {
            show_left_zone: true,
            show_right_zone: true,
            show_meter_bridge: true,
            show_channel_racks: true,
            show_history: false,
            show_control_room: true,
            strip_width: 188.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MixerRackVisibility {
    pub input: bool,
    pub pre: bool,
    pub strip: bool,
    pub eq: bool,
    pub inserts: bool,
    pub sends: bool,
    pub cues: bool,
}

impl Default for MixerRackVisibility {
    fn default() -> Self {
        Self {
            input: true,
            pre: true,
            strip: true,
            eq: true,
            inserts: true,
            sends: true,
            cues: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InsertSlot {
    pub name: String,
    pub bypass: bool,
    pub plugin_uid: Option<String>,
    pub format: Option<PluginFormat>,
    pub position: InsertPosition,
    pub sidechains: Vec<SidechainInput>,
    pub delay_comp_samples: u32,
}

impl InsertSlot {
    pub fn empty() -> Self {
        Self {
            name: "Empty".into(),
            bypass: false,
            plugin_uid: None,
            format: None,
            position: InsertPosition::default(),
            sidechains: Vec::new(),
            delay_comp_samples: 0,
        }
    }

    pub fn with_plugin(
        name: impl Into<String>,
        uid: impl Into<String>,
        format: PluginFormat,
    ) -> Self {
        Self {
            name: name.into(),
            bypass: false,
            plugin_uid: Some(uid.into()),
            format: Some(format),
            position: InsertPosition::default(),
            sidechains: Vec::new(),
            delay_comp_samples: 0,
        }
    }

    pub fn add_sidechain(&mut self, source: ChannelId, bus: impl Into<String>, pre_fader: bool) {
        self.sidechains.push(SidechainInput {
            source,
            bus: bus.into(),
            pre_fader,
        });
    }
}

#[derive(Clone, Debug)]
pub struct CueSend {
    pub id: SendId,
    pub name: String,
    pub level: f32,
    pub enabled: bool,
    pub pre_fader: bool,
}

#[derive(Clone, Debug)]
pub struct SendSlot {
    pub id: SendId, // index, rendered as 'A','B',...
    pub level: f32, // 0..1 linear
    pub pre_fader: bool,
}

#[derive(Clone, Debug)]
pub struct SidechainInput {
    pub source: ChannelId,
    pub bus: String,
    pub pre_fader: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqFilterKind {
    LowCut,
    LowShelf,
    Peak,
    HighShelf,
    HighCut,
}

impl EqFilterKind {
    pub fn label(&self) -> &'static str {
        match self {
            EqFilterKind::LowCut => "LowCut",
            EqFilterKind::LowShelf => "LowShelf",
            EqFilterKind::Peak => "Peak",
            EqFilterKind::HighShelf => "HighShelf",
            EqFilterKind::HighCut => "HighCut",
        }
    }
}

impl Default for EqFilterKind {
    fn default() -> Self {
        Self::Peak
    }
}

#[derive(Clone, Debug)]
pub struct EqBand {
    pub enabled: bool,
    pub kind: EqFilterKind,
    pub frequency_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

impl Default for EqBand {
    fn default() -> Self {
        Self {
            enabled: true,
            kind: EqFilterKind::Peak,
            frequency_hz: 1000.0,
            gain_db: 0.0,
            q: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChannelEq {
    pub enabled: bool,
    pub bands: Vec<EqBand>,
    pub analyzer_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct ChannelStripModules {
    pub drive: f32,
    pub gate_enabled: bool,
    pub compressor: f32,
    pub saturation: f32,
    pub limiter_enabled: bool,
}

impl Default for ChannelStripModules {
    fn default() -> Self {
        Self {
            drive: 0.0,
            gate_enabled: false,
            compressor: 0.0,
            saturation: 0.0,
            limiter_enabled: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChannelRackState {
    pub input_expanded: bool,
    pub pre_expanded: bool,
    pub strip_expanded: bool,
    pub eq_expanded: bool,
    pub inserts_expanded: bool,
    pub sends_expanded: bool,
    pub cues_expanded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationMode {
    Off,
    Read,
    Touch,
    Latch,
    Write,
}

impl AutomationMode {
    pub fn label(&self) -> &'static str {
        match self {
            AutomationMode::Off => "Off",
            AutomationMode::Read => "Read",
            AutomationMode::Touch => "Touch",
            AutomationMode::Latch => "Latch",
            AutomationMode::Write => "Write",
        }
    }
}

impl Default for AutomationMode {
    fn default() -> Self {
        Self::Read
    }
}

#[derive(Clone, Debug)]
pub struct Meter {
    /// instantaneous peak (linear 0..1), smoothed UI-side
    pub peak_l: f32,
    pub peak_r: f32,
    /// running RMS (linear 0..1), optional
    pub rms_l: f32,
    pub rms_r: f32,
    /// hold for peak display
    pub peak_hold_l: f32,
    pub peak_hold_r: f32,
    /// latched clip flags
    pub clip_l: bool,
    pub clip_r: bool,
    pub last_update: Instant,
}
impl Default for Meter {
    fn default() -> Self {
        Self {
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
            peak_hold_l: 0.0,
            peak_hold_r: 0.0,
            clip_l: false,
            clip_r: false,
            last_update: Instant::now(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub id: ChannelId,
    pub track_number: u16,
    pub name: String,
    pub gain_db: f32, // -60..+12
    pub pan: f32,     // -1..1
    pub mute: bool,
    pub solo: bool,
    pub inserts: Vec<InsertSlot>,
    pub sends: Vec<SendSlot>,
    pub meter: Meter,
    pub meter_history: VecDeque<f32>,
    pub is_master: bool,
    pub visible: bool,
    pub color: [u8; 3],
    pub input_bus: String,
    pub output_bus: String,
    pub record_enable: bool,
    pub monitor_enable: bool,
    pub phase_invert: bool,
    pub automation: AutomationMode,
    pub pre_gain_db: f32,
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub quick_controls: [f32; 8],
    pub cue_sends: Vec<CueSend>,
    pub eq: ChannelEq,
    pub strip_modules: ChannelStripModules,
    pub rack_state: ChannelRackState,
    pub inserts_delay_comp: u32,
    pub pan_law: PanLaw,
    pub stereo_separation: f32,
}

impl Channel {
    pub const METER_HISTORY_CAPACITY: usize = 64;

    pub fn new_meter_history() -> VecDeque<f32> {
        let mut history = VecDeque::with_capacity(Self::METER_HISTORY_CAPACITY);
        history.resize(Self::METER_HISTORY_CAPACITY, 0.0);
        history
    }

    pub fn ensure_insert_slot(&mut self, index: usize) {
        while self.inserts.len() <= index && self.inserts.len() < MAX_INSERT_SLOTS {
            self.inserts.push(InsertSlot::empty());
        }
    }

    pub fn insert_plugin(
        &mut self,
        index: usize,
        name: impl Into<String>,
        uid: impl Into<String>,
        format: PluginFormat,
        position: InsertPosition,
    ) -> Option<()> {
        self.ensure_insert_slot(index);
        let slot = self.inserts.get_mut(index)?;
        *slot = InsertSlot::with_plugin(name, uid, format);
        slot.position = position;
        Some(())
    }

    pub fn toggle_insert_bypass(&mut self, index: usize) {
        if let Some(slot) = self.inserts.get_mut(index) {
            slot.bypass = !slot.bypass;
        }
    }

    pub fn configure_send(&mut self, id: SendId, level: f32, pre_fader: bool) {
        if self.sends.len() < MAX_SEND_SLOTS {
            for new_id in self.sends.len() as u8..MAX_SEND_SLOTS as u8 {
                self.sends.push(SendSlot {
                    id: new_id,
                    level: 0.0,
                    pre_fader: false,
                });
            }
        }
        if let Some(slot) = self.sends.iter_mut().find(|s| s.id == id) {
            slot.level = level.clamp(0.0, 1.0);
            slot.pre_fader = pre_fader;
        }
    }
}

pub struct MixerState {
    pub channels: Vec<Channel>,
    pub selected: Option<ChannelId>,
    pub routing_visible: bool,
    pub routing: RoutingMatrix,
    pub view_tab: MixerViewTab,
    pub layout: MixerLayout,
    pub rack_visibility: MixerRackVisibility,
    pub channel_filter: String,
    pub master: MasterProcessing,
    pub default_pan_law: PanLaw,
    pub rack_routes: HashMap<u16, usize>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
            selected: None,
            routing_visible: false,
            routing: RoutingMatrix::default(),
            view_tab: MixerViewTab::default(),
            layout: MixerLayout::default(),
            rack_visibility: MixerRackVisibility::default(),
            channel_filter: String::new(),
            master: MasterProcessing::default(),
            default_pan_law: PanLaw::default(),
            rack_routes: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoutingMatrix {
    pub routes: HashMap<ChannelId, HashMap<String, f32>>,
}

#[derive(Clone, Debug)]
pub struct MasterProcessing {
    pub gain_db: f32,
    pub limiter_enabled: bool,
    pub dither_on_export: bool,
}

impl Default for MasterProcessing {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            limiter_enabled: true,
            dither_on_export: true,
        }
    }
}

impl RoutingMatrix {
    pub fn level(&self, channel: ChannelId, bus: &str) -> Option<f32> {
        self.routes
            .get(&channel)
            .and_then(|buses| buses.get(bus).copied())
    }

    pub fn set(&mut self, channel: ChannelId, bus: String, level: f32) {
        self.routes
            .entry(channel)
            .or_default()
            .insert(bus, level.clamp(0.0, 1.0));
    }

    pub fn remove(&mut self, channel: ChannelId, bus: &str) {
        if let Some(buses) = self.routes.get_mut(&channel) {
            buses.remove(bus);
            if buses.is_empty() {
                self.routes.remove(&channel);
            }
        }
    }

    pub fn apply_delta(&mut self, delta: &RoutingDelta) {
        for (channel, bus, level) in &delta.set {
            self.set(*channel, bus.clone(), *level);
        }
        for (channel, bus) in &delta.remove {
            self.remove(*channel, bus);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoutingDelta {
    pub set: Vec<(ChannelId, String, f32)>,
    pub remove: Vec<(ChannelId, String)>,
}

impl MixerState {
    pub fn new_default() -> Self {
        let mut s = Self::default();
        // 4 demo channels + master
        for i in 0..4 {
            s.channels.push(Channel {
                id: (i + 1) as ChannelId,
                name: format!("CH {}", i + 1),
                gain_db: 0.0,
                pan: 0.0,
                mute: false,
                solo: false,
                inserts: vec![InsertSlot::empty(); MAX_INSERT_SLOTS],
                sends: (0..MAX_SEND_SLOTS)
                    .map(|id| SendSlot {
                        id: id as u8,
                        level: 0.0,
                        pre_fader: false,
                    })
                    .collect(),
                meter: Meter::default(),
                meter_history: Channel::new_meter_history(),
                is_master: false,
                visible: true,
                color: [96, 156, 255],
                input_bus: format!("Input {}", i + 1),
                output_bus: "Stereo Out".into(),
                record_enable: false,
                monitor_enable: false,
                phase_invert: false,
                automation: AutomationMode::default(),
                pre_gain_db: 0.0,
                low_cut_hz: 20.0,
                high_cut_hz: 20000.0,
                quick_controls: [0.5; 8],
                cue_sends: vec![CueSend {
                    id: 0,
                    name: "Cue 1".into(),
                    level: 0.0,
                    enabled: false,
                    pre_fader: false,
                }],
                eq: ChannelEq {
                    enabled: true,
                    bands: vec![
                        EqBand {
                            kind: EqFilterKind::LowCut,
                            frequency_hz: 60.0,
                            gain_db: 0.0,
                            q: 0.7,
                            enabled: true,
                        },
                        EqBand {
                            kind: EqFilterKind::Peak,
                            frequency_hz: 200.0,
                            gain_db: 0.0,
                            q: 1.2,
                            enabled: true,
                        },
                        EqBand {
                            kind: EqFilterKind::Peak,
                            frequency_hz: 1200.0,
                            gain_db: 0.0,
                            q: 1.0,
                            enabled: true,
                        },
                        EqBand {
                            kind: EqFilterKind::HighShelf,
                            frequency_hz: 8000.0,
                            gain_db: 0.0,
                            q: 0.7,
                            enabled: true,
                        },
                    ],
                    analyzer_enabled: true,
                },
                strip_modules: ChannelStripModules::default(),
                rack_state: ChannelRackState {
                    input_expanded: true,
                    pre_expanded: true,
                    strip_expanded: true,
                    eq_expanded: true,
                    inserts_expanded: true,
                    sends_expanded: true,
                    cues_expanded: true,
                },
                inserts_delay_comp: 0,
                pan_law: PanLaw::ConstantPower,
                stereo_separation: 1.0,
            });
        }
        s.channels.push(Channel {
            id: 10_000,
            name: "MASTER".into(),
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            inserts: vec![InsertSlot::empty(); MAX_INSERT_SLOTS],
            sends: vec![],
            meter: Meter::default(),
            meter_history: Channel::new_meter_history(),
            is_master: true,
            visible: true,
            color: [252, 200, 64],
            input_bus: "MixBus".into(),
            output_bus: "Main Out".into(),
            record_enable: false,
            monitor_enable: false,
            phase_invert: false,
            automation: AutomationMode::default(),
            pre_gain_db: 0.0,
            low_cut_hz: 20.0,
            high_cut_hz: 20000.0,
            quick_controls: [0.5; 8],
            cue_sends: vec![],
            eq: ChannelEq {
                enabled: true,
                bands: vec![EqBand {
                    kind: EqFilterKind::HighShelf,
                    frequency_hz: 12000.0,
                    gain_db: 0.0,
                    q: 0.8,
                    enabled: true,
                }],
                analyzer_enabled: true,
            },
            strip_modules: ChannelStripModules::default(),
            rack_state: ChannelRackState {
                input_expanded: true,
                pre_expanded: true,
                strip_expanded: true,
                eq_expanded: true,
                inserts_expanded: true,
                sends_expanded: true,
                cues_expanded: false,
            },
            inserts_delay_comp: 0,
            pan_law: PanLaw::ConstantPower,
            stereo_separation: 1.0,
        });
        s
    }

    /// Host can call this from the non-RT copy that receives metering from the engine.
    /// Values are linear 0..(+ safety headroom) and will be clamped to 1.0 for UI.
    pub fn update_meter(
        &mut self,
        ch: ChannelId,
        peak_l: f32,
        peak_r: f32,
        rms_l: f32,
        rms_r: f32,
        clip_l: bool,
        clip_r: bool,
    ) {
        if let Some(c) = self.channels.iter_mut().find(|c| c.id == ch) {
            let (pl, pr, rl, rr) = (
                peak_l.clamp(0.0, 2.0),
                peak_r.clamp(0.0, 2.0),
                rms_l.clamp(0.0, 2.0),
                rms_r.clamp(0.0, 2.0),
            );
            // Simple smoothing: UI side 20ms coef
            let a = 0.2;
            c.meter.peak_l = c.meter.peak_l * (1.0 - a) + pl * a;
            c.meter.peak_r = c.meter.peak_r * (1.0 - a) + pr * a;
            c.meter.rms_l = c.meter.rms_l * (1.0 - a) + rl * a;
            c.meter.rms_r = c.meter.rms_r * (1.0 - a) + rr * a;
            c.meter.peak_hold_l = c.meter.peak_hold_l.max(pl).clamp(0.0, 1.5);
            c.meter.peak_hold_r = c.meter.peak_hold_r.max(pr).clamp(0.0, 1.5);
            c.meter.clip_l |= clip_l;
            c.meter.clip_r |= clip_r;
            c.meter.last_update = Instant::now();

            let rms_sample = ((c.meter.rms_l + c.meter.rms_r) * 0.5).clamp(0.0, 1.2);
            if c.meter_history.len() >= Channel::METER_HISTORY_CAPACITY {
                c.meter_history.pop_front();
            }
            c.meter_history.push_back(rms_sample.min(1.0));
        }
    }

    pub fn set_delay_comp_samples(&mut self, channel: ChannelId, samples: u32) {
        if let Some(ch) = self.channels.iter_mut().find(|c| c.id == channel) {
            ch.inserts_delay_comp = samples;
        }
    }

    pub fn reset_peaks_for(&mut self, channel: ChannelId) {
        if let Some(ch) = self.channels.iter_mut().find(|c| c.id == channel) {
            ch.meter.peak_hold_l = 0.0;
            ch.meter.peak_hold_r = 0.0;
            ch.meter.clip_l = false;
            ch.meter.clip_r = false;
        }
    }

    pub fn reset_peaks_all(&mut self) {
        for ch in &mut self.channels {
            ch.meter.peak_hold_l = 0.0;
            ch.meter.peak_hold_r = 0.0;
            ch.meter.clip_l = false;
            ch.meter.clip_r = false;
        }
    }

    pub fn set_default_pan_law(&mut self, pan_law: PanLaw) {
        self.default_pan_law = pan_law;
        for ch in &mut self.channels {
            ch.pan_law = pan_law;
        }
    }

    pub fn set_master_dither(&mut self, enabled: bool) {
        self.master.dither_on_export = enabled;
    }

    pub fn set_master_gain(&mut self, gain_db: f32) {
        self.master.gain_db = gain_db;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_send_configuration_respects_limits() {
        let mut state = MixerState::new_default();
        let first = state.channels.first_mut().unwrap();
        assert_eq!(first.inserts.len(), MAX_INSERT_SLOTS);
        assert_eq!(first.sends.len(), MAX_SEND_SLOTS);

        first
            .insert_plugin(
                5,
                "Compressor",
                "uid://comp",
                PluginFormat::Vst3,
                InsertPosition::PreFader,
            )
            .unwrap();
        assert_eq!(first.inserts[5].name, "Compressor");
        assert_eq!(first.inserts[5].format, Some(PluginFormat::Vst3));

        first.configure_send(2, 0.75, true);
        assert_eq!(first.sends[2].level, 0.75);
        assert!(first.sends[2].pre_fader);
    }

    #[test]
    fn master_processing_and_pan_law_can_be_changed() {
        let mut state = MixerState::new_default();
        state.set_default_pan_law(PanLaw::Minus3Db);
        assert!(state.channels.iter().all(|c| c.pan_law == PanLaw::Minus3Db));

        state.set_master_gain(-3.0);
        state.set_master_dither(false);
        assert_eq!(state.master.gain_db, -3.0);
        assert!(!state.master.dither_on_export);
    }
}
