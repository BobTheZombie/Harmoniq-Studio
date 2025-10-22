use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PanLaw {
    Linear,
    Minus3dB,
    Minus4Point5dB,
}

impl Default for PanLaw {
    fn default() -> Self {
        PanLaw::Minus3dB
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MixerMsg {
    SetGain { ch: usize, db: f32 },
    SetPan { ch: usize, pan: f32 },
    SetMute { ch: usize, on: bool },
    SetSolo { ch: usize, on: bool },
    SetPhaseInvert { ch: usize, on: bool },
    SetMono { ch: usize, on: bool },
    SetStereoLink { ch: usize, on: bool },
    SetInsertBypass { ch: usize, slot: usize, on: bool },
    SetInsertPostFader { ch: usize, slot: usize, post: bool },
    SetRoute { src: usize, dst: usize, on: bool },
    SetSendGain { src: usize, dst: usize, db: f32 },
    SetPanLaw { law: PanLaw },
    ResetPeaks,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MixerStateSnapshot {
    pub meter_peak_lr: Vec<(f32, f32)>,
    pub meter_rms_lr: Vec<(f32, f32)>,
    pub latencies: Vec<u32>,
    pub cpu_load_per_core: Vec<f32>,
}

impl MixerStateSnapshot {
    pub fn with_channel_capacity(capacity: usize, cores: usize) -> Self {
        let mut snapshot = MixerStateSnapshot::default();
        snapshot
            .meter_peak_lr
            .resize(capacity, (f32::NEG_INFINITY, f32::NEG_INFINITY));
        snapshot
            .meter_rms_lr
            .resize(capacity, (f32::NEG_INFINITY, f32::NEG_INFINITY));
        snapshot.latencies.resize(capacity, 0);
        snapshot.cpu_load_per_core.resize(cores.max(1), 0.0);
        snapshot
    }
}

#[derive(Debug, Clone)]
pub struct MixerBus {
    pub tx: Sender<MixerMsg>,
    pub rx_snap: Receiver<MixerStateSnapshot>,
}

#[derive(Debug, Clone)]
pub struct MixerEngineEndpoint {
    pub rx: Receiver<MixerMsg>,
    pub tx_snap: Sender<MixerStateSnapshot>,
}

pub fn create_mixer_bus(capacity: usize) -> (MixerBus, MixerEngineEndpoint) {
    let (tx_msg, rx_msg) = bounded::<MixerMsg>(capacity);
    let (tx_snap, rx_snap) = bounded::<MixerStateSnapshot>(capacity.max(2));

    let ui = MixerBus {
        tx: tx_msg,
        rx_snap,
    };
    let engine = MixerEngineEndpoint {
        rx: rx_msg,
        tx_snap,
    };

    (ui, engine)
}

pub const SEND_COUNT: usize = 4;
