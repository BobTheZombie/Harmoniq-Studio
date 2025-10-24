use std::sync::Arc;

use harmoniq_dsp::gain::db_to_linear;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MixerTargetState {
    Master,
    Bus(usize),
}

impl Default for MixerTargetState {
    fn default() -> Self {
        MixerTargetState::Master
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerTrackState {
    pub name: String,
    pub fader_db: f32,
    pub width: f32,
    pub phase_invert: bool,
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
            target: MixerTargetState::Master,
            aux_sends: Vec::new(),
            pre_inserts: Vec::new(),
            post_inserts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerBusState {
    pub name: String,
    pub fader_db: f32,
    pub width: f32,
    pub phase_invert: bool,
    pub aux_sends: Vec<MixerAuxSendState>,
    pub post_inserts: Vec<MixerInsertState>,
}

impl Default for MixerBusState {
    fn default() -> Self {
        Self {
            name: "Bus".into(),
            fader_db: 0.0,
            width: 1.0,
            phase_invert: false,
            aux_sends: Vec::new(),
            post_inserts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerMasterState {
    pub fader_db: f32,
    pub width: f32,
    pub phase_invert: bool,
}

impl Default for MixerMasterState {
    fn default() -> Self {
        Self {
            fader_db: 0.0,
            width: 1.0,
            phase_invert: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    ) {
        if input.is_empty() {
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
    sends: Vec<TrackSend>,
    post_inserts: Vec<Option<Arc<Mutex<Box<dyn MixerInsertProcessor>>>>>,
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
        Self {
            fader,
            width,
            meter,
            sends,
            post_inserts,
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        master: &mut AudioBuffer,
        aux_buffers: &mut [AudioBuffer],
    ) {
        if buffer.is_empty() {
            return;
        }
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
        add_scaled(master, buffer, 1.0);
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
        for (engine, input) in self.tracks.iter_mut().zip(track_inputs.iter()) {
            engine.process(input, &mut self.bus_buffers, output, &mut self.aux_buffers);
        }
        for (bus_engine, buffer) in self.buses.iter_mut().zip(self.bus_buffers.iter_mut()) {
            bus_engine.process(buffer, output, &mut self.aux_buffers);
        }
        for (aux_engine, buffer) in self.auxes.iter_mut().zip(self.aux_buffers.iter_mut()) {
            aux_engine.process(buffer, output);
        }
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
