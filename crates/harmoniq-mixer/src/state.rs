use std::collections::HashMap;
use std::time::Instant;

pub type ChannelId = u32;
pub type InsertSlotId = usize;
pub type SendId = u8; // 0='A',1='B',...

#[derive(Clone, Debug)]
pub struct InsertSlot {
    pub name: String,
    pub bypass: bool,
}

#[derive(Clone, Debug)]
pub struct SendSlot {
    pub id: SendId, // index, rendered as 'A','B',...
    pub level: f32, // 0..1 linear
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
    pub is_master: bool,
}

pub struct MixerState {
    pub channels: Vec<Channel>,
    pub selected: Option<ChannelId>,
    pub routing_visible: bool,
    pub routing: RoutingMatrix,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
            selected: None,
            routing_visible: false,
            routing: RoutingMatrix::default(),
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
                is_master: false,
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
            is_master: true,
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
