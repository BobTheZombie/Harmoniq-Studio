use std::collections::VecDeque;
use std::sync::Arc;

pub mod api;
#[cfg(feature = "mixer_api")]
pub mod control;
pub mod levels;

// CURRENT ARCH SUMMARY:
// - MixerEngine processes tracks->buses->master with pre/post inserts and aux returns.
// - State structs hold mixer layout for serialization but lack strong track typing and RT/editor split.
// - Meter taps exist per strip, driven by MixerEngine, with UI helpers in `mixer/api.rs`.
// - Missing: Cubase-style channel roles, explicit mute/solo/arm/monitor flow, pan law control,
//   and RT-safe parameter bridging between UI/editor and the audio graph.

// Mixer Architecture Plan:
// - Introduce typed channels (audio, group, fx, master) with shared flags (mute/solo/arm/monitor/polarity).
// - Push pan/pan-law handling into the engine-side mixer and expose automation-friendly targets.
// - Keep RT structs allocation-free; use editor-side state to drive atomics/lock-free updates into DSP.
// - Expand MixerEngine to gate signals with mute/solo logic, apply pan/width, and mix sends/routes
//   before master processing, mirroring Cubase-style signal flow.

use harmoniq_dsp::gain::db_to_linear;
use harmoniq_dsp::pan::constant_power;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::buffer::AudioBuffer;
use crate::dsp::nodes::{FaderNode, MeterHandle, MeterTapNode, StereoWidthNode};

/// Runtime audio processor that can be inserted into a mixer channel.
pub trait MixerInsertProcessor: Send {
    fn process(&mut self, buffer: &mut AudioBuffer);
}

impl<T> MixerInsertProcessor for T
where
    T: FnMut(&mut AudioBuffer) + Send,
{
    fn process(&mut self, buffer: &mut AudioBuffer) {
        self(buffer);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerInsertState {
    pub id: Option<String>,
    pub bypassed: bool,
}

impl Default for MixerInsertState {
    fn default() -> Self {
        Self {
            id: None,
            bypassed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerAuxSendState {
    pub aux_index: usize,
    pub level_db: f32,
    pub pre_fader: bool,
}

impl Default for MixerAuxSendState {
    fn default() -> Self {
        Self {
            aux_index: 0,
            level_db: -60.0,
            pre_fader: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MixerTargetState {
    Master,
    Bus(usize),
}

impl Default for MixerTargetState {
    fn default() -> Self {
        MixerTargetState::Master
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MixerTrackType {
    Audio,
    Group,
    FxReturn,
    Master,
}

impl Default for MixerTrackType {
    fn default() -> Self {
        MixerTrackType::Audio
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerTrackState {
    pub name: String,
    pub fader_db: f32,
    pub width: f32,
    pub phase_invert: bool,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub record_arm: bool,
    pub monitor: bool,
    pub track_type: MixerTrackType,
    pub input_bus: Option<String>,
    pub output_bus: Option<String>,
    pub target: MixerTargetState,
    pub aux_sends: Vec<MixerAuxSendState>,
    pub pre_inserts: Vec<MixerInsertState>,
    pub post_inserts: Vec<MixerInsertState>,
}

impl Default for MixerTrackState {
    fn default() -> Self {
        Self {
            name: "Track".into(),
            fader_db: 0.0,
            width: 1.0,
            phase_invert: false,
            pan: 0.0,
            mute: false,
            solo: false,
            record_arm: false,
            monitor: false,
            track_type: MixerTrackType::Audio,
            input_bus: None,
            output_bus: None,
            target: MixerTargetState::Master,
            aux_sends: Vec::new(),
            pre_inserts: Vec::new(),
            post_inserts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerBusState {
    pub name: String,
    pub fader_db: f32,
    pub width: f32,
    pub phase_invert: bool,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub aux_sends: Vec<MixerAuxSendState>,
    pub post_inserts: Vec<MixerInsertState>,
    pub target: MixerTargetState,
}

impl Default for MixerBusState {
    fn default() -> Self {
        Self {
            name: "Bus".into(),
            fader_db: 0.0,
            width: 1.0,
            phase_invert: false,
            pan: 0.0,
            mute: false,
            solo: false,
            aux_sends: Vec::new(),
            post_inserts: Vec::new(),
            target: MixerTargetState::Master,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerAuxState {
    pub name: String,
    pub return_db: f32,
}

impl Default for MixerAuxState {
    fn default() -> Self {
        Self {
            name: "Aux".into(),
            return_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerMasterState {
    pub fader_db: f32,
    pub width: f32,
    pub phase_invert: bool,
    pub pan: f32,
}

impl Default for MixerMasterState {
    fn default() -> Self {
        Self {
            fader_db: 0.0,
            width: 1.0,
            phase_invert: false,
            pan: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MixerState {
    pub tracks: Vec<MixerTrackState>,
    pub buses: Vec<MixerBusState>,
    pub auxes: Vec<MixerAuxState>,
    pub master: MixerMasterState,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            tracks: vec![
                MixerTrackState {
                    name: "Drums".into(),
                    ..MixerTrackState::default()
                },
                MixerTrackState {
                    name: "Bass".into(),
                    ..MixerTrackState::default()
                },
                MixerTrackState {
                    name: "Lead".into(),
                    ..MixerTrackState::default()
                },
                MixerTrackState {
                    name: "Pads".into(),
                    ..MixerTrackState::default()
                },
            ],
            buses: Vec::new(),
            auxes: vec![MixerAuxState {
                name: "Hall".into(),
                return_db: -6.0,
            }],
            master: MixerMasterState::default(),
        }
    }
}

#[derive(Clone)]
pub struct MixerModel {
    state: MixerState,
    track_meters: Vec<MeterHandle>,
    bus_meters: Vec<MeterHandle>,
    aux_meters: Vec<MeterHandle>,
    master_meter: MeterHandle,
    pre_inserts: Vec<Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>>,
    post_inserts: Vec<Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>>,
    bus_post_inserts: Vec<Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>>,
}

impl MixerModel {
    pub fn new(state: MixerState) -> Self {
        let mut model = Self {
            track_meters: Vec::new(),
            bus_meters: Vec::new(),
            aux_meters: Vec::new(),
            master_meter: MeterHandle::new(),
            pre_inserts: Vec::new(),
            post_inserts: Vec::new(),
            bus_post_inserts: Vec::new(),
            state,
        };
        model.ensure_handles();
        model
    }

    pub fn state(&self) -> &MixerState {
        &self.state
    }

    pub fn into_state(self) -> MixerState {
        self.state
    }

    pub fn track_meter(&self, index: usize) -> Option<MeterHandle> {
        self.track_meters.get(index).cloned()
    }

    pub fn bus_meter(&self, index: usize) -> Option<MeterHandle> {
        self.bus_meters.get(index).cloned()
    }

    pub fn aux_meter(&self, index: usize) -> Option<MeterHandle> {
        self.aux_meters.get(index).cloned()
    }

    pub fn master_meter(&self) -> MeterHandle {
        self.master_meter.clone()
    }

    pub fn set_state(&mut self, state: MixerState) {
        self.state = state;
        self.ensure_handles();
    }

    fn ensure_handles(&mut self) {
        if self.track_meters.len() != self.state.tracks.len() {
            self.track_meters = self
                .state
                .tracks
                .iter()
                .map(|_| MeterHandle::new())
                .collect();
        }
        if self.bus_meters.len() != self.state.buses.len() {
            self.bus_meters = self
                .state
                .buses
                .iter()
                .map(|_| MeterHandle::new())
                .collect();
        }
        if self.aux_meters.len() != self.state.auxes.len() {
            self.aux_meters = self
                .state
                .auxes
                .iter()
                .map(|_| MeterHandle::new())
                .collect();
        }
        self.pre_inserts = self
            .state
            .tracks
            .iter()
            .map(|track| vec![None; track.pre_inserts.len()])
            .collect();
        self.post_inserts = self
            .state
            .tracks
            .iter()
            .map(|track| vec![None; track.post_inserts.len()])
            .collect();
        self.bus_post_inserts = self
            .state
            .buses
            .iter()
            .map(|bus| vec![None; bus.post_inserts.len()])
            .collect();
    }

    pub fn set_track_pre_insert(
        &mut self,
        track: usize,
        slot: usize,
        insert: Option<Box<dyn MixerInsertProcessor>>,
    ) {
        if let Some(slots) = self.pre_inserts.get_mut(track) {
            if let Some(slot_ref) = slots.get_mut(slot) {
                *slot_ref = insert.map(|proc| Arc::new(Mutex::new(proc)));
            }
        }
    }

    pub fn set_track_post_insert(
        &mut self,
        track: usize,
        slot: usize,
        insert: Option<Box<dyn MixerInsertProcessor>>,
    ) {
        if let Some(slots) = self.post_inserts.get_mut(track) {
            if let Some(slot_ref) = slots.get_mut(slot) {
                *slot_ref = insert.map(|proc| Arc::new(Mutex::new(proc)));
            }
        }
    }

    pub fn set_bus_post_insert(
        &mut self,
        bus: usize,
        slot: usize,
        insert: Option<Box<dyn MixerInsertProcessor>>,
    ) {
        if let Some(slots) = self.bus_post_inserts.get_mut(bus) {
            if let Some(slot_ref) = slots.get_mut(slot) {
                *slot_ref = insert.map(|proc| Arc::new(Mutex::new(proc)));
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MixerTarget {
    Master,
    Bus(usize),
}

struct TrackSend {
    aux: usize,
    gain: f32,
    pre_fader: bool,
}

struct TrackEngine {
    fader: FaderNode,
    width: StereoWidthNode,
    meter: MeterTapNode,
    pan: f32,
    mute: bool,
    solo: bool,
    track_type: MixerTrackType,
    record_arm: bool,
    monitor: bool,
    target: MixerTarget,
    sends: Vec<TrackSend>,
    pre_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
    post_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
    pre_buffer: AudioBuffer,
    post_buffer: AudioBuffer,
}

impl TrackEngine {
    fn new(
        state: &MixerTrackState,
        handle: MeterHandle,
        pre_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
        post_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
        sample_rate: f32,
    ) -> Self {
        let mut fader = FaderNode::new(state.fader_db);
        fader.set_phase_invert(state.phase_invert);
        fader.prepare(sample_rate);
        let mut meter = MeterTapNode::new(sample_rate, handle);
        meter.prepare(sample_rate, 2);
        let target = match state.target {
            MixerTargetState::Master => MixerTarget::Master,
            MixerTargetState::Bus(index) => MixerTarget::Bus(index),
        };
        let sends = state
            .aux_sends
            .iter()
            .map(|send| TrackSend {
                aux: send.aux_index,
                gain: db_to_linear(send.level_db),
                pre_fader: send.pre_fader,
            })
            .collect();
        let mut width = StereoWidthNode::new(state.width);
        width.set_width(state.width);
        Self {
            fader,
            width,
            meter,
            pan: state.pan,
            mute: state.mute,
            solo: state.solo,
            track_type: state.track_type.clone(),
            record_arm: state.record_arm,
            monitor: state.monitor,
            target,
            sends,
            pre_inserts,
            post_inserts,
            pre_buffer: AudioBuffer::new(2, 0),
            post_buffer: AudioBuffer::new(2, 0),
        }
    }

    fn ensure_capacity(&mut self, channels: usize, frames: usize) {
        self.pre_buffer.resize(channels, frames);
        self.post_buffer.resize(channels, frames);
    }

    fn process(
        &mut self,
        input: &AudioBuffer,
        buses: &mut [AudioBuffer],
        master: &mut AudioBuffer,
        aux_buffers: &mut [AudioBuffer],
        any_solo: bool,
    ) {
        if input.is_empty() {
            return;
        }
        if self.mute || (any_solo && !self.solo) {
            return;
        }
        let channels = input.channel_count();
        let frames = input.len();
        self.ensure_capacity(channels, frames);
        self.pre_buffer
            .as_mut_slice()
            .copy_from_slice(input.as_slice());
        for proc_opt in self.pre_inserts.iter() {
            if let Some(processor) = proc_opt {
                if let Some(mut guard) = processor.try_lock() {
                    guard.process(&mut self.pre_buffer);
                }
            }
        }
        self.post_buffer
            .as_mut_slice()
            .copy_from_slice(self.pre_buffer.as_slice());
        apply_pan(&mut self.post_buffer, self.pan);
        self.width.process_buffer(&mut self.post_buffer);
        self.fader.process_buffer(&mut self.post_buffer);
        for proc_opt in self.post_inserts.iter() {
            if let Some(processor) = proc_opt {
                if let Some(mut guard) = processor.try_lock() {
                    guard.process(&mut self.post_buffer);
                }
            }
        }
        self.meter.process_buffer(&self.post_buffer);
        for send in &self.sends {
            let source = if send.pre_fader {
                &self.pre_buffer
            } else {
                &self.post_buffer
            };
            if let Some(aux) = aux_buffers.get_mut(send.aux) {
                add_scaled(aux, source, send.gain);
            }
        }
        match self.target {
            MixerTarget::Master => add_scaled(master, &self.post_buffer, 1.0),
            MixerTarget::Bus(index) => {
                if let Some(bus) = buses.get_mut(index) {
                    add_scaled(bus, &self.post_buffer, 1.0);
                }
            }
        }
    }
}

struct BusEngine {
    fader: FaderNode,
    width: StereoWidthNode,
    meter: MeterTapNode,
    pan: f32,
    mute: bool,
    solo: bool,
    sends: Vec<TrackSend>,
    post_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
    target: MixerTarget,
}

impl BusEngine {
    fn new(
        state: &MixerBusState,
        handle: MeterHandle,
        post_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
        sample_rate: f32,
    ) -> Self {
        let mut fader = FaderNode::new(state.fader_db);
        fader.set_phase_invert(state.phase_invert);
        fader.prepare(sample_rate);
        let mut meter = MeterTapNode::new(sample_rate, handle);
        meter.prepare(sample_rate, 2);
        let sends = state
            .aux_sends
            .iter()
            .map(|send| TrackSend {
                aux: send.aux_index,
                gain: db_to_linear(send.level_db),
                pre_fader: send.pre_fader,
            })
            .collect();
        let mut width = StereoWidthNode::new(state.width);
        width.set_width(state.width);
        let target = match state.target {
            MixerTargetState::Master => MixerTarget::Master,
            MixerTargetState::Bus(index) => MixerTarget::Bus(index),
        };
        Self {
            fader,
            width,
            meter,
            pan: state.pan,
            mute: state.mute,
            solo: state.solo,
            sends,
            post_inserts,
            target,
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        aux_buffers: &mut [AudioBuffer],
        any_solo: bool,
    ) {
        if buffer.is_empty() {
            return;
        }
        if self.mute || (any_solo && !self.solo) {
            return;
        }
        apply_pan(buffer, self.pan);
        self.width.process_buffer(buffer);
        self.fader.process_buffer(buffer);
        for proc_opt in self.post_inserts.iter() {
            if let Some(processor) = proc_opt {
                if let Some(mut guard) = processor.try_lock() {
                    guard.process(buffer);
                }
            }
        }
        self.meter.process_buffer(buffer);
        for send in &self.sends {
            if let Some(aux) = aux_buffers.get_mut(send.aux) {
                add_scaled(aux, buffer, send.gain);
            }
        }
    }

    fn target(&self) -> MixerTarget {
        self.target
    }
}

struct AuxEngine {
    fader: FaderNode,
    meter: MeterTapNode,
}

impl AuxEngine {
    fn new(state: &MixerAuxState, handle: MeterHandle, sample_rate: f32) -> Self {
        let mut fader = FaderNode::new(state.return_db);
        fader.prepare(sample_rate);
        let mut meter = MeterTapNode::new(sample_rate, handle);
        meter.prepare(sample_rate, 2);
        Self { fader, meter }
    }

    fn process(&mut self, buffer: &mut AudioBuffer, master: &mut AudioBuffer) {
        if buffer.is_empty() {
            return;
        }
        self.fader.process_buffer(buffer);
        self.meter.process_buffer(buffer);
        add_scaled(master, buffer, 1.0);
        buffer.clear();
    }
}

pub struct MixerEngine {
    tracks: Vec<TrackEngine>,
    buses: Vec<BusEngine>,
    auxes: Vec<AuxEngine>,
    bus_buffers: Vec<AudioBuffer>,
    aux_buffers: Vec<AudioBuffer>,
    master_fader: FaderNode,
    master_width: StereoWidthNode,
    master_meter: MeterTapNode,
    master_pan: f32,
}

impl MixerEngine {
    pub fn from_model(model: &MixerModel, sample_rate: f32, block_size: usize) -> Self {
        let tracks = model
            .state
            .tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                let handle = model.track_meters[idx].clone();
                TrackEngine::new(
                    track,
                    handle,
                    model.pre_inserts.get(idx).cloned().unwrap_or_default(),
                    model.post_inserts.get(idx).cloned().unwrap_or_default(),
                    sample_rate,
                )
            })
            .collect();
        let buses = model
            .state
            .buses
            .iter()
            .enumerate()
            .map(|(idx, bus)| {
                let handle = model.bus_meters[idx].clone();
                BusEngine::new(
                    bus,
                    handle,
                    model.bus_post_inserts.get(idx).cloned().unwrap_or_default(),
                    sample_rate,
                )
            })
            .collect();
        let auxes = model
            .state
            .auxes
            .iter()
            .enumerate()
            .map(|(idx, aux)| AuxEngine::new(aux, model.aux_meters[idx].clone(), sample_rate))
            .collect();
        let mut master_fader = FaderNode::new(model.state.master.fader_db);
        master_fader.set_phase_invert(model.state.master.phase_invert);
        master_fader.prepare(sample_rate);
        let mut master_meter = MeterTapNode::new(sample_rate, model.master_meter.clone());
        master_meter.prepare(sample_rate, 2);
        let mut master_width = StereoWidthNode::new(model.state.master.width);
        master_width.set_width(model.state.master.width);
        Self {
            tracks,
            buses,
            auxes,
            bus_buffers: vec![AudioBuffer::new(2, block_size); model.state.buses.len()],
            aux_buffers: vec![AudioBuffer::new(2, block_size); model.state.auxes.len()],
            master_fader,
            master_width,
            master_meter,
            master_pan: model.state.master.pan,
        }
    }

    pub fn process(&mut self, track_inputs: &[AudioBuffer], output: &mut AudioBuffer) {
        assert_eq!(track_inputs.len(), self.tracks.len());
        let channels = track_inputs
            .first()
            .map(|buffer| buffer.channel_count())
            .unwrap_or(2)
            .max(1);
        let frames = track_inputs.first().map(|buffer| buffer.len()).unwrap_or(0);
        if output.channel_count() != channels || output.len() != frames {
            output.resize(channels, frames);
        }
        output.clear();
        for buffer in &mut self.bus_buffers {
            if buffer.channel_count() != channels || buffer.len() != frames {
                buffer.resize(channels, frames);
            }
            buffer.clear();
        }
        for buffer in &mut self.aux_buffers {
            if buffer.channel_count() != channels || buffer.len() != frames {
                buffer.resize(channels, frames);
            }
            buffer.clear();
        }
        let any_track_solo = self.tracks.iter().any(|t| t.solo);
        for (engine, input) in self.tracks.iter_mut().zip(track_inputs.iter()) {
            engine.process(
                input,
                &mut self.bus_buffers,
                output,
                &mut self.aux_buffers,
                any_track_solo,
            );
        }
        if !self.buses.is_empty() {
            let mut indegree = vec![0usize; self.buses.len()];
            for bus_index in 0..self.buses.len() {
                if let MixerTarget::Bus(target) = self.buses[bus_index].target() {
                    if target < indegree.len() {
                        indegree[target] += 1;
                    }
                }
            }
            let mut queue: VecDeque<usize> = indegree
                .iter()
                .enumerate()
                .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
                .collect();
            let mut order = Vec::with_capacity(self.buses.len());
            while let Some(index) = queue.pop_front() {
                order.push(index);
                if let MixerTarget::Bus(target) = self.buses[index].target() {
                    if target < indegree.len() {
                        indegree[target] = indegree[target].saturating_sub(1);
                        if indegree[target] == 0 {
                            queue.push_back(target);
                        }
                    }
                }
            }

            let any_bus_solo = self.buses.iter().any(|b| b.solo);
            for index in order {
                let target = self.buses[index].target();
                let mut buffer = std::mem::take(&mut self.bus_buffers[index]);
                {
                    let bus_engine = &mut self.buses[index];
                    bus_engine.process(&mut buffer, &mut self.aux_buffers, any_bus_solo);
                }
                match target {
                    MixerTarget::Master => add_scaled(output, &buffer, 1.0),
                    MixerTarget::Bus(target_idx) => {
                        if target_idx < self.bus_buffers.len() {
                            add_scaled(&mut self.bus_buffers[target_idx], &buffer, 1.0);
                        }
                    }
                }
                self.bus_buffers[index] = buffer;
            }
        }
        for (aux_engine, buffer) in self.auxes.iter_mut().zip(self.aux_buffers.iter_mut()) {
            aux_engine.process(buffer, output);
        }
        apply_pan(output, self.master_pan);
        self.master_width.process_buffer(output);
        self.master_fader.process_buffer(output);
        self.master_meter.process_buffer(output);
    }
}

fn add_scaled(target: &mut AudioBuffer, source: &AudioBuffer, gain: f32) {
    if target.is_empty() || source.is_empty() {
        return;
    }
    let frames = target.len().min(source.len());
    let channels = target.channel_count().min(source.channel_count());
    let tgt_stride = target.len();
    let src_stride = source.len();
    let tgt = target.as_mut_slice();
    let src = source.as_slice();
    for ch in 0..channels {
        let t_offset = ch * tgt_stride;
        let s_offset = ch * src_stride;
        for frame in 0..frames {
            let t = t_offset + frame;
            let s = s_offset + frame;
            tgt[t] += src[s] * gain;
        }
    }
}

fn apply_pan(buffer: &mut AudioBuffer, pan: f32) {
    if buffer.channel_count() == 0 || buffer.is_empty() {
        return;
    }
    let (g_l, g_r) = constant_power(pan.clamp(-1.0, 1.0));
    let frames = buffer.len();
    let channels = buffer.channel_count();
    let data = buffer.as_mut_slice();
    let left_offset = 0;
    for frame in 0..frames {
        let idx = left_offset + frame;
        data[idx] *= g_l;
    }
    if channels > 1 {
        let right_offset = frames;
        for frame in 0..frames {
            let idx = right_offset + frame;
            data[idx] *= g_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_with_value(channels: usize, frames: usize, value: f32) -> AudioBuffer {
        let mut buf = AudioBuffer::new(channels, frames);
        buf.as_mut_slice().fill(value);
        buf
    }

    #[test]
    fn pre_fader_aux_reaches_master_mix() {
        let mut track = MixerTrackState::default();
        track.name = "Track 1".into();
        track.fader_db = -60.0;
        track.target = MixerTargetState::Bus(0);
        track.aux_sends.push(MixerAuxSendState {
            aux_index: 0,
            level_db: 0.0,
            pre_fader: true,
        });

        let mut bus = MixerBusState::default();
        bus.target = MixerTargetState::Master;

        let aux = MixerAuxState::default();

        let state = MixerState {
            tracks: vec![track],
            buses: vec![bus],
            auxes: vec![aux],
            master: MixerMasterState::default(),
        };

        let model = MixerModel::new(state);
        let mut engine = MixerEngine::from_model(&model, 48_000.0, 16);

        let input = buffer_with_value(2, 8, 1.0);
        let mut output = AudioBuffer::new(2, 8);
        engine.process(&[input], &mut output);

        let track_linear = db_to_linear(-60.0);
        for sample in output.as_slice() {
            assert!((*sample) > 0.99, "pre-fader aux should dominate master mix");
            assert!((sample - (1.0 + track_linear)).abs() < 0.1);
        }
    }

    #[test]
    fn post_fader_aux_tracks_fader_level() {
        let mut track = MixerTrackState::default();
        track.fader_db = -12.0;
        track.target = MixerTargetState::Bus(0);
        track.aux_sends.push(MixerAuxSendState {
            aux_index: 0,
            level_db: 0.0,
            pre_fader: false,
        });

        let mut bus = MixerBusState::default();
        bus.target = MixerTargetState::Master;

        let state = MixerState {
            tracks: vec![track],
            buses: vec![bus],
            auxes: vec![MixerAuxState::default()],
            master: MixerMasterState::default(),
        };

        let model = MixerModel::new(state);
        let mut engine = MixerEngine::from_model(&model, 48_000.0, 16);

        let input = buffer_with_value(2, 4, 1.0);
        let mut output = AudioBuffer::new(2, 4);
        engine.process(&[input], &mut output);

        let expected = db_to_linear(-12.0) * 2.0; // bus + post-fader aux return
        for sample in output.as_slice() {
            assert!((sample - expected).abs() < 0.02);
        }
    }
}
