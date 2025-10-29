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

#[derive(Default)]
pub struct MixerState {
    pub channels: Vec<Channel>,
    pub selected: Option<ChannelId>,
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
            c.meter.last_update = Instant::now();
        }
    }
}
