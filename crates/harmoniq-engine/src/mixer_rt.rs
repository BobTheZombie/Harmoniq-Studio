use arc_swap::ArcSwap;
use atomic_float::AtomicF32;
use core::sync::atomic::Ordering;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::collections::BTreeMap;
use std::sync::Arc;

pub type TrackId = u16;

/// Commands written by the **non-RT** (engine/UI) thread and drained by the audio thread.
#[derive(Clone, Debug)]
pub enum Command {
    SetGain {
        track: TrackId,
        gain_db: f32,
    },
    SetPan {
        track: TrackId,
        pan: f32,
    },
    SetMute {
        track: TrackId,
        mute: bool,
    },
    SetSolo {
        track: TrackId,
        solo: bool,
    },
    SetMasterGain {
        gain_db: f32,
    },
    EnableTrack {
        track: TrackId,
        enable: bool,
    },
    /// Routing swap: atomically switch to a new routing table at next block.
    SwapRouting {
        epoch_hint: u64,
        table: Arc<RoutingTable>,
    },
}

pub type CommandTx = HeapProducer<Command>;
pub type CommandRx = HeapConsumer<Command>;

/// Automation events drained by the audio thread at the start of each block.
#[derive(Clone, Debug)]
pub enum AutomationEvent {
    /// Linear ramp from the current gain target to `to` over `duration` samples.
    GainDbRamp {
        track: TrackId,
        to: f32,
        duration: u32,
    },
    /// Linear ramp from the current pan target to `to` over `duration` samples.
    PanRamp {
        track: TrackId,
        to: f32,
        duration: u32,
    },
    /// Linear ramp for the master gain target.
    MasterGainDbRamp { to: f32, duration: u32 },
}

pub type AutoTx = HeapProducer<AutomationEvent>;
pub type AutoRx = HeapConsumer<AutomationEvent>;

#[derive(Clone, Copy, Debug)]
pub struct MixerConfig {
    pub max_tracks: usize,
    pub max_block: usize,
    pub sample_rate: f32,
    /// One-pole smoothing coefficient in [0, 1].
    pub smooth_alpha: f32,
    /// Maximum number of aux busses to pre-allocate buffers for.
    pub max_aux_busses: usize,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            max_tracks: 64,
            max_block: 1024,
            sample_rate: 48_000.0,
            smooth_alpha: 0.2,
            max_aux_busses: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RampState {
    remaining: u32,
    step: f32,
    target: f32,
}

impl RampState {
    fn reset(&mut self) {
        self.remaining = 0;
        self.step = 0.0;
        self.target = 0.0;
    }

    fn start(&mut self, current: f32, target: f32, duration: u32) {
        if duration == 0 {
            self.remaining = 0;
            self.step = 0.0;
            self.target = target;
            return;
        }
        self.remaining = duration;
        self.target = target;
        self.step = (target - current) / duration as f32;
    }

    fn is_active(&self) -> bool {
        self.remaining > 0
    }

    fn advance(&mut self, value: &mut f32) {
        if self.remaining == 0 {
            return;
        }
        self.remaining -= 1;
        if self.remaining == 0 {
            *value = self.target;
            self.step = 0.0;
        } else {
            *value += self.step;
        }
    }
}

#[derive(Debug)]
struct Track {
    enabled: bool,
    gain_target_lin: AtomicF32,
    pan_target: AtomicF32,
    mute: AtomicF32,
    solo: AtomicF32,
    gain_work_lin: f32,
    pan_work: f32,
    gain_target_current: f32,
    pan_target_current: f32,
    gain_ramp: RampState,
    pan_ramp: RampState,
    peak_atomic: AtomicF32,
    rms_atomic: AtomicF32,
    peak_block: f32,
    rms_accum: f64,
    rms_count: usize,
}

impl Track {
    fn new() -> Self {
        Self {
            enabled: false,
            gain_target_lin: AtomicF32::new(1.0),
            pan_target: AtomicF32::new(0.0),
            mute: AtomicF32::new(0.0),
            solo: AtomicF32::new(0.0),
            gain_work_lin: 1.0,
            pan_work: 0.0,
            gain_target_current: 1.0,
            pan_target_current: 0.0,
            gain_ramp: RampState::default(),
            pan_ramp: RampState::default(),
            peak_atomic: AtomicF32::new(0.0),
            rms_atomic: AtomicF32::new(0.0),
            peak_block: 0.0,
            rms_accum: 0.0,
            rms_count: 0,
        }
    }
}

/// Aux bus identifiers.
pub type AuxBusId = u16;
/// Group identifiers.
pub type GroupId = u16;

/// Fixed routing table that can be swapped atomically at block boundaries.
#[derive(Debug)]
pub struct RoutingTable {
    pub sends: Vec<Vec<(AuxBusId, f32)>>,
    pub group_of: Vec<Option<usize>>,
    pub groups: Vec<Group>,
    pub aux_to_master_gain: Vec<f32>,
}

#[derive(Debug)]
pub struct Group {
    pub id: GroupId,
    pub members: Vec<usize>,
}

pub struct RoutingBuilder {
    max_tracks: usize,
    aux_count: usize,
    sends: Vec<Vec<(AuxBusId, f32)>>,
    group_of: Vec<Option<GroupId>>,
    aux_to_master_gain: Vec<f32>,
}

impl RoutingBuilder {
    pub fn new(max_tracks: usize, aux_count: usize) -> Self {
        Self {
            max_tracks,
            aux_count,
            sends: vec![Vec::new(); max_tracks],
            group_of: vec![None; max_tracks],
            aux_to_master_gain: vec![1.0; aux_count],
        }
    }

    pub fn send(mut self, track: usize, aux: AuxBusId, gain_lin: f32) -> Self {
        debug_assert!(track < self.max_tracks);
        debug_assert!((aux as usize) < self.aux_count);
        self.sends[track].push((aux, gain_lin));
        self
    }

    pub fn group(mut self, track: usize, group: GroupId) -> Self {
        debug_assert!(track < self.max_tracks);
        self.group_of[track] = Some(group);
        self
    }

    pub fn aux_gain(mut self, aux: AuxBusId, gain_lin: f32) -> Self {
        if let Some(slot) = self.aux_to_master_gain.get_mut(aux as usize) {
            *slot = gain_lin;
        }
        self
    }

    pub fn build(self) -> Arc<RoutingTable> {
        let mut groups_map: BTreeMap<GroupId, Vec<usize>> = BTreeMap::new();
        for (idx, group) in self.group_of.iter().enumerate() {
            if let Some(gid) = group {
                groups_map.entry(*gid).or_default().push(idx);
            }
        }
        let mut lookup: BTreeMap<GroupId, usize> = BTreeMap::new();
        let mut groups = Vec::with_capacity(groups_map.len());
        for (idx, (gid, members)) in groups_map.into_iter().enumerate() {
            lookup.insert(gid, idx);
            groups.push(Group { id: gid, members });
        }
        let mut group_indices = vec![None; self.max_tracks];
        for (track_idx, maybe_gid) in self.group_of.into_iter().enumerate() {
            if let Some(gid) = maybe_gid {
                if let Some(&group_idx) = lookup.get(&gid) {
                    group_indices[track_idx] = Some(group_idx);
                }
            }
        }
        Arc::new(RoutingTable {
            sends: self.sends,
            group_of: group_indices,
            groups,
            aux_to_master_gain: self.aux_to_master_gain,
        })
    }
}

pub struct Mixer {
    cfg: MixerConfig,
    tracks: Vec<Track>,
    master_gain_target: AtomicF32,
    master_gain_work: f32,
    master_target_current: f32,
    master_ramp: RampState,
    rx: CommandRx,
    auto_rx: AutoRx,
    left_accum: Vec<f32>,
    right_accum: Vec<f32>,
    aux_l: Vec<f32>,
    aux_r: Vec<f32>,
    group_l: Vec<f32>,
    group_r: Vec<f32>,
    routing_epoch: ArcSwap<RoutingTable>,
    routing_shadow: Arc<RoutingTable>,
}

impl Mixer {
    /// Create mixer with preallocated scratch buffers and lock-free control queues.
    /// Returns (mixer, command_tx, automation_tx).
    pub fn new(
        cfg: MixerConfig,
        cmd_capacity: usize,
        auto_capacity: usize,
    ) -> (Self, CommandTx, AutoTx) {
        let cmd_rb = HeapRb::<Command>::new(cmd_capacity);
        let (cmd_tx, rx) = cmd_rb.split();
        let auto_rb = HeapRb::<AutomationEvent>::new(auto_capacity);
        let (auto_tx, auto_rx) = auto_rb.split();

        let mut tracks = Vec::with_capacity(cfg.max_tracks);
        tracks.resize_with(cfg.max_tracks, Track::new);

        let left_accum = vec![0.0f32; cfg.max_block];
        let right_accum = vec![0.0f32; cfg.max_block];
        let aux_capacity = cfg.max_aux_busses.max(1);
        let aux_l = vec![0.0f32; aux_capacity * cfg.max_block];
        let aux_r = vec![0.0f32; aux_capacity * cfg.max_block];
        let group_l = vec![0.0f32; cfg.max_tracks * cfg.max_block];
        let group_r = vec![0.0f32; cfg.max_tracks * cfg.max_block];

        let routing = RoutingBuilder::new(cfg.max_tracks, cfg.max_aux_busses).build();

        (
            Self {
                cfg,
                tracks,
                master_gain_target: AtomicF32::new(1.0),
                master_gain_work: 1.0,
                master_target_current: 1.0,
                master_ramp: RampState::default(),
                rx,
                auto_rx,
                left_accum: left_accum,
                right_accum: right_accum,
                aux_l,
                aux_r,
                group_l,
                group_r,
                routing_epoch: ArcSwap::from(routing.clone()),
                routing_shadow: routing,
            },
            cmd_tx,
            auto_tx,
        )
    }

    fn apply_cmd(&mut self, cmd: Command) {
        match cmd {
            Command::SetGain { track, gain_db } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    let lin = db_to_lin(gain_db);
                    t.gain_target_lin.store(lin, Ordering::Relaxed);
                    t.gain_target_current = lin;
                    t.gain_ramp.reset();
                }
            }
            Command::SetPan { track, pan } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    let clamped = pan.clamp(-1.0, 1.0);
                    t.pan_target.store(clamped, Ordering::Relaxed);
                    t.pan_target_current = clamped;
                    t.pan_ramp.reset();
                }
            }
            Command::SetMute { track, mute } => {
                if let Some(t) = self.tracks.get(track as usize) {
                    t.mute
                        .store(if mute { 1.0 } else { 0.0 }, Ordering::Relaxed);
                }
            }
            Command::SetSolo { track, solo } => {
                if let Some(t) = self.tracks.get(track as usize) {
                    t.solo
                        .store(if solo { 1.0 } else { 0.0 }, Ordering::Relaxed);
                }
            }
            Command::SetMasterGain { gain_db } => {
                let lin = db_to_lin(gain_db);
                self.master_gain_target.store(lin, Ordering::Relaxed);
                self.master_target_current = lin;
                self.master_ramp.reset();
            }
            Command::EnableTrack { track, enable } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    t.enabled = enable;
                }
            }
            Command::SwapRouting { table, .. } => {
                debug_assert!(table.aux_to_master_gain.len() <= self.cfg.max_aux_busses);
                debug_assert!(table.groups.len() <= self.cfg.max_tracks);
                self.routing_epoch.store(table);
            }
        }
    }

    /// Drain the command and automation queues. **Call on the audio thread** once per block.
    pub fn begin_block(&mut self) {
        while let Some(cmd) = self.rx.pop() {
            self.apply_cmd(cmd);
        }

        while let Some(ev) = self.auto_rx.pop() {
            match ev {
                AutomationEvent::GainDbRamp {
                    track,
                    to,
                    duration,
                } => {
                    if let Some(t) = self.tracks.get_mut(track as usize) {
                        let to_lin = db_to_lin(to);
                        t.gain_target_lin.store(to_lin, Ordering::Relaxed);
                        if duration == 0 {
                            t.gain_target_current = to_lin;
                            t.gain_ramp.reset();
                        } else {
                            let start = t.gain_target_current;
                            t.gain_ramp.start(start, to_lin, duration);
                        }
                    }
                }
                AutomationEvent::PanRamp {
                    track,
                    to,
                    duration,
                } => {
                    if let Some(t) = self.tracks.get_mut(track as usize) {
                        let clamped = to.clamp(-1.0, 1.0);
                        t.pan_target.store(clamped, Ordering::Relaxed);
                        if duration == 0 {
                            t.pan_target_current = clamped;
                            t.pan_ramp.reset();
                        } else {
                            let start = t.pan_target_current;
                            t.pan_ramp.start(start, clamped, duration);
                        }
                    }
                }
                AutomationEvent::MasterGainDbRamp { to, duration } => {
                    let to_lin = db_to_lin(to);
                    self.master_gain_target.store(to_lin, Ordering::Relaxed);
                    if duration == 0 {
                        self.master_target_current = to_lin;
                        self.master_ramp.reset();
                    } else {
                        let start = self.master_target_current;
                        self.master_ramp.start(start, to_lin, duration);
                    }
                }
            }
        }

        self.routing_shadow = self.routing_epoch.load_full();
    }

    /// Mix mono per-track inputs into stereo outputs.
    ///
    /// * `inputs` must contain `cfg.max_tracks` entries (unused tracks can be `None`).
    /// * `out_l` and `out_r` must have length `nframes` (<= `cfg.max_block`).
    pub fn process(
        &mut self,
        inputs: &[Option<&[f32]>],
        out_l: &mut [f32],
        out_r: &mut [f32],
        nframes: usize,
    ) {
        debug_assert!(nframes <= self.cfg.max_block);
        debug_assert_eq!(out_l.len(), nframes);
        debug_assert_eq!(out_r.len(), nframes);

        self.left_accum[..nframes].fill(0.0);
        self.right_accum[..nframes].fill(0.0);

        for track in &mut self.tracks {
            track.peak_block = 0.0;
            track.rms_accum = 0.0;
            track.rms_count = 0;
        }

        let aux_count = self
            .routing_shadow
            .aux_to_master_gain
            .len()
            .min(self.cfg.max_aux_busses);
        for aux_idx in 0..aux_count {
            let base = aux_idx * self.cfg.max_block;
            self.aux_l[base..base + nframes].fill(0.0);
            self.aux_r[base..base + nframes].fill(0.0);
        }

        let group_count = self.routing_shadow.groups.len().min(self.cfg.max_tracks);
        for group_idx in 0..group_count {
            let base = group_idx * self.cfg.max_block;
            self.group_l[base..base + nframes].fill(0.0);
            self.group_r[base..base + nframes].fill(0.0);
        }

        let any_solo = self
            .tracks
            .iter()
            .any(|t| t.enabled && t.solo.load(Ordering::Relaxed) >= 0.5);

        for (ti, track) in self.tracks.iter_mut().enumerate() {
            if !track.enabled {
                continue;
            }
            let Some(input) = inputs.get(ti).and_then(|slot| *slot) else {
                continue;
            };

            let mute = track.mute.load(Ordering::Relaxed) >= 0.5;
            let solo_this = track.solo.load(Ordering::Relaxed) >= 0.5;
            if mute || (any_solo && !solo_this) {
                continue;
            }

            let n = nframes.min(input.len());
            let group_idx = self
                .routing_shadow
                .group_of
                .get(ti)
                .and_then(|g| *g)
                .filter(|idx| *idx < group_count);
            for i in 0..n {
                if track.gain_ramp.is_active() {
                    track.gain_ramp.advance(&mut track.gain_target_current);
                }
                if track.pan_ramp.is_active() {
                    track.pan_ramp.advance(&mut track.pan_target_current);
                }

                let target_gain = track.gain_target_current;
                let target_pan = track.pan_target_current;
                track.gain_work_lin += (target_gain - track.gain_work_lin) * self.cfg.smooth_alpha;
                track.pan_work += (target_pan - track.pan_work) * self.cfg.smooth_alpha;

                let sample = input[i] * track.gain_work_lin;
                let (l, r) = pan_mono(sample, track.pan_work);
                if let Some(group_idx) = group_idx {
                    let base = group_idx * self.cfg.max_block;
                    self.group_l[base + i] += l;
                    self.group_r[base + i] += r;
                } else {
                    self.left_accum[i] += l;
                    self.right_accum[i] += r;
                }

                let abs_l = l.abs();
                let abs_r = r.abs();
                let peak = abs_l.max(abs_r);
                if peak > track.peak_block {
                    track.peak_block = peak;
                }

                let mono = (l + r) * 0.5;
                track.rms_accum += (mono as f64) * (mono as f64);
                track.rms_count += 1;
            }

            if let Some(sends) = self.routing_shadow.sends.get(ti) {
                for &(aux_id, send_gain) in sends.iter() {
                    let aux_idx = aux_id as usize;
                    if aux_idx >= aux_count {
                        continue;
                    }
                    let base = aux_idx * self.cfg.max_block;
                    for i in 0..n {
                        let send_sample = input[i] * track.gain_work_lin * send_gain;
                        let (l, r) = pan_mono(send_sample, track.pan_work);
                        self.aux_l[base + i] += l;
                        self.aux_r[base + i] += r;
                    }
                }
            }
        }

        for aux_idx in 0..aux_count {
            let gain = self.routing_shadow.aux_to_master_gain[aux_idx];
            let base = aux_idx * self.cfg.max_block;
            for i in 0..nframes {
                self.left_accum[i] += self.aux_l[base + i] * gain;
                self.right_accum[i] += self.aux_r[base + i] * gain;
            }
        }

        for group_idx in 0..group_count {
            let base = group_idx * self.cfg.max_block;
            for i in 0..nframes {
                self.left_accum[i] += self.group_l[base + i];
                self.right_accum[i] += self.group_r[base + i];
            }
        }

        for i in 0..nframes {
            if self.master_ramp.is_active() {
                self.master_ramp.advance(&mut self.master_target_current);
            }
            self.master_gain_work +=
                (self.master_target_current - self.master_gain_work) * self.cfg.smooth_alpha;
            let master = self.master_gain_work;
            out_l[i] = self.left_accum[i] * master;
            out_r[i] = self.right_accum[i] * master;
        }
    }

    /// Finalize block processing (publish meters).
    pub fn end_block(&mut self) {
        let rms_decay = 0.9f32;
        for track in &mut self.tracks {
            track.peak_atomic.store(track.peak_block, Ordering::Relaxed);
            if track.rms_count > 0 {
                let block_rms = (track.rms_accum / track.rms_count as f64).sqrt() as f32;
                let previous = track.rms_atomic.load(Ordering::Relaxed);
                track.rms_atomic.store(
                    previous * rms_decay + block_rms * (1.0 - rms_decay),
                    Ordering::Relaxed,
                );
            } else {
                let previous = track.rms_atomic.load(Ordering::Relaxed);
                track
                    .rms_atomic
                    .store(previous * rms_decay, Ordering::Relaxed);
            }
        }
    }

    /// Read the most recent peak meter for a track.
    pub fn track_peak(&self, track: TrackId) -> Option<f32> {
        self.tracks
            .get(track as usize)
            .map(|t| t.peak_atomic.load(Ordering::Relaxed))
    }

    /// Read the most recent RMS meter for a track.
    pub fn track_rms(&self, track: TrackId) -> Option<f32> {
        self.tracks
            .get(track as usize)
            .map(|t| t.rms_atomic.load(Ordering::Relaxed))
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    if db <= -90.0 {
        0.0
    } else {
        (10.0f32).powf(db * 0.05)
    }
}

#[inline]
fn pan_mono(sample: f32, pan: f32) -> (f32, f32) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5;
    let angle = core::f32::consts::FRAC_PI_2 * p;
    let l = angle.cos();
    let r = angle.sin();
    (sample * l, sample * r)
}
