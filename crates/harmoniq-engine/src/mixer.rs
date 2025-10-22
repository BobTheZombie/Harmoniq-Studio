use std::time::{Duration, Instant};

use crossbeam_channel::{select, Sender};
use harmoniq_app::mixer::rt_api::{
    MixerEngineEndpoint, MixerMsg, MixerStateSnapshot, PanLaw, SEND_COUNT,
};

const SNAPSHOT_RATE: Duration = Duration::from_millis(16);

pub struct MixerEngine {
    stop_tx: Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl MixerEngine {
    pub fn new(endpoint: MixerEngineEndpoint, channel_count: usize) -> Self {
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let worker = std::thread::Builder::new()
            .name("harmoniq-mixer-engine".to_string())
            .spawn(move || {
                let MixerEngineEndpoint { rx, tx_snap } = endpoint;
                let mut state = MixerEngineState::new(channel_count);
                let mut snapshot = MixerStateSnapshot::with_channel_capacity(
                    state.total_channels(),
                    available_cores(),
                );
                let mut next_tick = Instant::now();

                loop {
                    let mut exit = false;
                    select! {
                        recv(stop_rx) -> _ => { exit = true; },
                        default(Duration::from_millis(0)) => {}
                    }
                    if exit {
                        break;
                    }

                    while let Ok(msg) = rx.try_recv() {
                        state.apply(msg);
                    }

                    let now = Instant::now();
                    if now >= next_tick {
                        state.fill_snapshot(&mut snapshot);
                        let _ = tx_snap.try_send(snapshot.clone());
                        next_tick = now + SNAPSHOT_RATE;
                    }

                    std::thread::sleep(Duration::from_millis(2));
                }
            })
            .expect("failed to spawn mixer engine thread");

        MixerEngine {
            stop_tx,
            worker: Some(worker),
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.stop_tx.send(());
            let _ = worker.join();
        }
    }
}

impl Drop for MixerEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct MixerEngineState {
    channels: Vec<ChannelRuntime>,
    routes: Vec<Vec<RouteRuntime>>,
    pan_law: PanLaw,
    start_time: Instant,
}

impl MixerEngineState {
    fn new(channel_count: usize) -> Self {
        let total = channel_count + SEND_COUNT + 1;
        let mut channels = Vec::with_capacity(total);
        for _ in 0..total {
            channels.push(ChannelRuntime::default());
        }

        let mut routes = vec![vec![RouteRuntime::default(); total]; channel_count + SEND_COUNT];
        let master_idx = total - 1;
        for src in 0..(channel_count + SEND_COUNT) {
            if let Some(route) = routes.get_mut(src).and_then(|row| row.get_mut(master_idx)) {
                route.enabled = true;
            }
        }

        MixerEngineState {
            channels,
            routes,
            pan_law: PanLaw::default(),
            start_time: Instant::now(),
        }
    }

    fn total_channels(&self) -> usize {
        self.channels.len()
    }

    fn apply(&mut self, msg: MixerMsg) {
        match msg {
            MixerMsg::SetGain { ch, db } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.gain_db = db;
                }
            }
            MixerMsg::SetPan { ch, pan } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.pan = pan;
                }
            }
            MixerMsg::SetMute { ch, on } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.mute = on;
                }
            }
            MixerMsg::SetSolo { ch, on } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.solo = on;
                }
            }
            MixerMsg::SetPhaseInvert { ch, on } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.phase_invert = on;
                }
            }
            MixerMsg::SetMono { ch, on } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.mono = on;
                }
            }
            MixerMsg::SetStereoLink { ch, on } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    channel.stereo_link = on;
                }
            }
            MixerMsg::SetInsertBypass { ch, slot, on } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    if let Some(insert) = channel.inserts.get_mut(slot) {
                        insert.bypass = on;
                    }
                }
            }
            MixerMsg::SetInsertPostFader { ch, slot, post } => {
                if let Some(channel) = self.channels.get_mut(ch) {
                    if let Some(insert) = channel.inserts.get_mut(slot) {
                        insert.post_fader = post;
                    }
                }
            }
            MixerMsg::SetRoute { src, dst, on } => {
                if let Some(route) = self.routes.get_mut(src).and_then(|row| row.get_mut(dst)) {
                    route.enabled = on;
                }
            }
            MixerMsg::SetSendGain { src, dst, db } => {
                if let Some(route) = self.routes.get_mut(src).and_then(|row| row.get_mut(dst)) {
                    route.send_gain_db = db;
                }
            }
            MixerMsg::SetPanLaw { law } => {
                self.pan_law = law;
            }
            MixerMsg::ResetPeaks => {
                for channel in &mut self.channels {
                    channel.meter_peak = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                }
            }
        }
    }

    fn fill_snapshot(&mut self, snapshot: &mut MixerStateSnapshot) {
        let total = self.total_channels();
        if snapshot.meter_peak_lr.len() != total {
            snapshot
                .meter_peak_lr
                .resize(total, (f32::NEG_INFINITY, f32::NEG_INFINITY));
            snapshot
                .meter_rms_lr
                .resize(total, (f32::NEG_INFINITY, f32::NEG_INFINITY));
            snapshot.latencies.resize(total, 0);
        }
        let elapsed = self.start_time.elapsed().as_secs_f32();
        for (index, channel) in self.channels.iter_mut().enumerate() {
            let phase = elapsed + index as f32 * 0.2;
            let signal = (phase.sin() * 0.5 + 0.5).powf(2.0);
            let db = linear_to_db(signal * 0.9 + 0.05);
            channel.meter_peak = (db, db - 1.5);
            channel.meter_rms = (db - 3.0, db - 3.0);
            snapshot.meter_peak_lr[index] = channel.meter_peak;
            snapshot.meter_rms_lr[index] = channel.meter_rms;
            snapshot.latencies[index] = channel.latency_samples;
        }
        if snapshot.cpu_load_per_core.is_empty() {
            snapshot.cpu_load_per_core = vec![0.08, 0.06];
        } else {
            for (idx, core) in snapshot.cpu_load_per_core.iter_mut().enumerate() {
                let phase = elapsed + idx as f32;
                *core = 0.05 + (phase.sin() * 0.02).abs();
            }
        }
    }
}

#[derive(Clone)]
struct ChannelRuntime {
    gain_db: f32,
    pan: f32,
    mute: bool,
    solo: bool,
    phase_invert: bool,
    mono: bool,
    stereo_link: bool,
    latency_samples: u32,
    inserts: Vec<InsertRuntime>,
    meter_peak: (f32, f32),
    meter_rms: (f32, f32),
}

impl Default for ChannelRuntime {
    fn default() -> Self {
        ChannelRuntime {
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            phase_invert: false,
            mono: false,
            stereo_link: false,
            latency_samples: 0,
            inserts: vec![InsertRuntime::default(); 10],
            meter_peak: (f32::NEG_INFINITY, f32::NEG_INFINITY),
            meter_rms: (f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }
}

#[derive(Clone)]
struct InsertRuntime {
    bypass: bool,
    post_fader: bool,
}

impl Default for InsertRuntime {
    fn default() -> Self {
        InsertRuntime {
            bypass: false,
            post_fader: false,
        }
    }
}

#[derive(Clone, Copy)]
struct RouteRuntime {
    enabled: bool,
    send_gain_db: f32,
}

impl Default for RouteRuntime {
    fn default() -> Self {
        RouteRuntime {
            enabled: false,
            send_gain_db: 0.0,
        }
    }
}

fn available_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn linear_to_db(value: f32) -> f32 {
    if value <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * value.log10()
    }
}
