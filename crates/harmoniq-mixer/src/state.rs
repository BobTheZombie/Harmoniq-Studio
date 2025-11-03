use std::collections::{HashMap, VecDeque};
use std::time::Instant;

pub type ChannelId = u32;
pub type InsertSlotId = usize;
pub type SendId = u8; // 0='A',1='B',...

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
}

impl Channel {
    pub const METER_HISTORY_CAPACITY: usize = 64;

    pub fn new_meter_history() -> VecDeque<f32> {
        let mut history = VecDeque::with_capacity(Self::METER_HISTORY_CAPACITY);
        history.resize(Self::METER_HISTORY_CAPACITY, 0.0);
        history
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
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoutingMatrix {
    pub routes: HashMap<ChannelId, HashMap<String, f32>>,
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
                inserts: Vec::new(),
                sends: vec![
                    SendSlot { id: 0, level: 0.0 },
                    SendSlot { id: 1, level: 0.0 },
                ],
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
            });
        }
        s.channels.push(Channel {
            id: 10_000,
            name: "MASTER".into(),
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            inserts: Vec::new(),
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
}
