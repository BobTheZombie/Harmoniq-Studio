use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam::queue::ArrayQueue;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;

use crate::mixer::api::{MixerUiApi, MixerUiState};
use crate::{
    automation::{AutomationEvent, AutomationLane, AutomationSender, ParameterSpec},
    graph::{self, GraphBuilder, GraphHandle},
    nodes::{GainNode as BuiltinGain, NodeNoise as BuiltinNoise, NodeOsc as BuiltinSine},
    plugin::{MidiEvent, PluginDescriptor, PluginId},
    rt::{AudioMetrics, AudioMetricsCollector},
    rt_bridge::RtBridge,
    scratch::RtAllocGuard,
    tone::ToneShaper,
    transport::Transport as TransportMetrics,
    AudioBuffer, AudioClip, AudioProcessor, BufferConfig,
};
use harmoniq_rt::RtEvent;

const COMMAND_QUEUE_CAPACITY: usize = 1024;
const METRICS_HISTORY_CAPACITY: usize = 512;

/// Transport state shared with UI and sequencing components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Playing,
    Recording,
}

impl Default for TransportState {
    fn default() -> Self {
        TransportState::Stopped
    }
}

/// Real-time safe command queue handle for communicating with the engine.
#[derive(Clone, Debug)]
pub struct EngineCommandQueue {
    queue: Arc<ArrayQueue<EngineCommand>>,
}

impl EngineCommandQueue {
    /// Attempts to push a command onto the queue without blocking.
    ///
    /// Returns the original command if the queue is full so callers can retry or
    /// degrade gracefully.
    pub fn try_send(&self, command: EngineCommand) -> Result<(), EngineCommand> {
        self.queue.push(command).map_err(|command| command)
    }

    /// Number of commands currently waiting to be processed.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Maximum capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Returns `true` when there are no pending commands.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Commands that can be sent to the engine from UI or automation.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    SetTempo(f32),
    SetTransport(TransportState),
    ReplaceGraph(GraphHandle),
    SubmitMidi(Vec<MidiEvent>),
    PlaySoundTest(AudioClip),
}

/// Central Harmoniq engine responsible for orchestrating the processing graph.
pub struct HarmoniqEngine {
    config: BufferConfig,
    processors: RwLock<HashMap<PluginId, Arc<Mutex<Box<dyn AudioProcessor>>>>>,
    graph: RwLock<Option<GraphHandle>>,
    master_buffer: Mutex<AudioBuffer>,
    tone_shaper: ToneShaper,
    next_plugin_id: AtomicU64,
    transport: RwLock<TransportState>,
    transport_metrics: Arc<TransportMetrics>,
    command_queue: Arc<ArrayQueue<EngineCommand>>,
    pending_midi: Mutex<Vec<MidiEvent>>,
    automations: RwLock<HashMap<PluginId, AutomationLane>>,
    latencies: RwLock<HashMap<PluginId, usize>>,
    delay_lines: HashMap<PluginId, DelayCompensator>,
    scratch_buffers: Vec<AudioBuffer>,
    sound_test: Option<ClipPlayback>,
    metrics: AudioMetricsCollector,
    block_period_ns: u64,
    automation_cursor: u64,
    mixer_ui: Arc<MixerUiState>,
    rt_bridge: Option<RtBridge>,
    last_reported_xruns: u64,
    last_reported_engine_load: u16,
    last_reported_max_block_us: u32,
}

impl HarmoniqEngine {
    pub fn new(config: BufferConfig) -> anyhow::Result<Self> {
        let command_queue = Arc::new(ArrayQueue::new(COMMAND_QUEUE_CAPACITY));
        let tone_shaper = ToneShaper::new(&config);
        let metrics = AudioMetricsCollector::new(METRICS_HISTORY_CAPACITY);
        let block_period_ns = Self::block_period_from_config(&config);
        let transport_metrics = Arc::new(TransportMetrics::default());
        transport_metrics
            .sr
            .store(config.sample_rate.round() as u32, Ordering::Relaxed);

        let mixer_ui = MixerUiState::demo();
        let mut engine = Self {
            master_buffer: Mutex::new(AudioBuffer::from_config(&config)),
            processors: RwLock::new(HashMap::new()),
            graph: RwLock::new(None),
            next_plugin_id: AtomicU64::new(1),
            transport: RwLock::new(TransportState::Stopped),
            transport_metrics: Arc::clone(&transport_metrics),
            command_queue,
            pending_midi: Mutex::new(Vec::new()),
            config,
            tone_shaper,
            automations: RwLock::new(HashMap::new()),
            latencies: RwLock::new(HashMap::new()),
            delay_lines: HashMap::new(),
            scratch_buffers: Vec::new(),
            sound_test: None,
            metrics,
            block_period_ns,
            automation_cursor: 0,
            mixer_ui,
            rt_bridge: None,
            last_reported_xruns: 0,
            last_reported_engine_load: 0,
            last_reported_max_block_us: 0,
        };
        engine.install_default_graph()?;
        Ok(engine)
    }

    pub fn install_rt_bridge(&mut self, bridge: RtBridge) {
        self.rt_bridge = Some(bridge);
        self.last_reported_xruns = 0;
        self.last_reported_engine_load = 0;
        self.last_reported_max_block_us = 0;
    }

    pub fn config(&self) -> &BufferConfig {
        &self.config
    }

    pub fn graph(&self) -> Option<GraphHandle> {
        self.graph.read().clone()
    }

    pub fn plugin_descriptor(&self, id: PluginId) -> Option<PluginDescriptor> {
        let processors = self.processors.read();
        let handle = processors.get(&id)?.clone();
        drop(processors);
        let descriptor = handle.lock().descriptor();
        Some(descriptor)
    }

    pub fn reset_render_state(&mut self) -> anyhow::Result<()> {
        self.pending_midi.lock().clear();
        self.transport_metrics
            .sample_pos
            .store(0, Ordering::Relaxed);
        self.transport_metrics
            .playing
            .store(false, Ordering::Relaxed);
        self.automation_cursor = 0;
        self.metrics.reset();
        self.last_reported_xruns = 0;
        self.last_reported_engine_load = 0;
        self.last_reported_max_block_us = 0;
        self.sound_test = None;
        self.master_buffer.lock().clear();
        for buffer in &mut self.scratch_buffers {
            buffer.clear();
        }
        for delay in self.delay_lines.values_mut() {
            delay.reset();
        }

        {
            let processors = self.processors.read();
            for processor in processors.values() {
                processor.lock().prepare(&self.config)?;
            }
        }

        Ok(())
    }

    pub fn metrics(&self) -> AudioMetrics {
        self.metrics.snapshot()
    }

    pub fn metrics_collector(&self) -> AudioMetricsCollector {
        self.metrics.clone()
    }

    pub fn mixer_ui_api(&self) -> Arc<dyn MixerUiApi> {
        Arc::clone(&self.mixer_ui) as Arc<dyn MixerUiApi>
    }

    fn block_period_from_config(config: &BufferConfig) -> u64 {
        if config.block_size == 0 {
            return 0;
        }
        let sr = config.sample_rate.max(f32::EPSILON) as f64;
        let frames = config.block_size as f64;
        ((frames / sr) * 1_000_000_000.0).round() as u64
    }

    fn install_default_graph(&mut self) -> anyhow::Result<()> {
        if self.graph.read().is_some() {
            return Ok(());
        }

        let sine =
            self.register_processor(Box::new(BuiltinSine::new(220.0).with_amplitude(0.35)))?;
        let noise = self.register_processor(Box::new(BuiltinNoise::new(0.08)))?;
        let gain = self.register_processor(Box::new(BuiltinGain::new(0.6)))?;
        self.register_automation_parameter(gain, ParameterSpec::new(0, "Gain", 0.0, 2.0, 0.6))?;

        let mut builder = GraphBuilder::new();
        let sine_node = builder.add_node(sine);
        builder.connect_to_mixer(sine_node, 0.85)?;
        let noise_node = builder.add_node(noise);
        builder.connect_to_mixer(noise_node, 0.25)?;
        let gain_node = builder.add_node(gain);
        builder.connect_to_mixer(gain_node, 0.0)?;

        self.replace_graph(builder.build())
    }

    pub fn reconfigure(&mut self, config: BufferConfig) -> anyhow::Result<()> {
        let tone_enabled = self.tone_shaper.is_enabled();
        self.config = config.clone();
        self.master_buffer = Mutex::new(AudioBuffer::from_config(&config));
        self.tone_shaper = ToneShaper::new(&self.config);
        self.tone_shaper.set_enabled(tone_enabled);
        self.block_period_ns = Self::block_period_from_config(&self.config);
        self.metrics.reset();
        self.transport_metrics
            .sr
            .store(self.config.sample_rate.round() as u32, Ordering::Relaxed);

        self.scratch_buffers.clear();

        {
            let processors = self.processors.read();
            let mut latencies = self.latencies.write();
            latencies.clear();
            for (id, processor) in processors.iter() {
                let mut processor = processor.lock();
                processor.prepare(&self.config)?;
                latencies.insert(*id, processor.latency_samples());
            }
        }

        self.delay_lines.clear();
        self.sound_test = None;
        Ok(())
    }

    /// Enables or disables the built-in tone shaper. By default the engine
    /// keeps the shaper bypassed so that the master bus remains sonically
    /// neutral when no additional effects are loaded.
    pub fn set_tone_shaper_enabled(&mut self, enabled: bool) {
        self.tone_shaper.set_enabled(enabled);
    }

    /// Returns a lightweight handle that can be shared with UI threads for
    /// submitting commands.
    pub fn command_queue(&self) -> EngineCommandQueue {
        EngineCommandQueue {
            queue: Arc::clone(&self.command_queue),
        }
    }

    /// Attempts to enqueue a command directly on the engine's queue.
    pub fn try_enqueue_command(&self, command: EngineCommand) -> Result<(), EngineCommand> {
        self.command_queue.push(command).map_err(|command| command)
    }

    pub fn transport(&self) -> TransportState {
        *self.transport.read()
    }

    pub fn set_transport(&mut self, state: TransportState) {
        let mut current = self.transport.write();
        let was_playing = matches!(
            *current,
            TransportState::Playing | TransportState::Recording
        );
        let now_playing = matches!(state, TransportState::Playing | TransportState::Recording);
        *current = state;
        drop(current);

        self.transport_metrics
            .playing
            .store(now_playing, Ordering::Relaxed);
        if now_playing && !was_playing {
            self.transport_metrics
                .sample_pos
                .store(0, Ordering::Relaxed);
            self.automation_cursor = 0;
        }
        if !now_playing {
            self.transport_metrics
                .playing
                .store(false, Ordering::Relaxed);
        }
    }

    pub fn transport_metrics(&self) -> Arc<TransportMetrics> {
        Arc::clone(&self.transport_metrics)
    }

    pub fn register_processor(
        &mut self,
        mut processor: Box<dyn AudioProcessor>,
    ) -> anyhow::Result<PluginId> {
        processor.prepare(&self.config)?;
        let latency = processor.latency_samples();
        let id = PluginId(self.next_plugin_id.fetch_add(1, Ordering::SeqCst));
        let descriptor = processor.descriptor();
        tracing::info!("Registered processor: {}", descriptor);
        let shared = Arc::new(Mutex::new(processor));
        self.processors.write().insert(id, shared);
        self.latencies.write().insert(id, latency);
        self.delay_lines.insert(id, DelayCompensator::new());
        let mut lanes = self.automations.write();
        lanes.insert(id, AutomationLane::new(id, 1024));
        Ok(id)
    }

    pub fn replace_graph(&self, graph: GraphHandle) -> anyhow::Result<()> {
        if graph.is_empty() {
            anyhow::bail!("graph must contain at least one node");
        }
        *self.graph.write() = Some(graph);
        Ok(())
    }

    pub fn execute_command(&mut self, command: EngineCommand) -> anyhow::Result<()> {
        self.handle_command(command)
    }

    fn handle_command(&mut self, command: EngineCommand) -> anyhow::Result<()> {
        match command {
            EngineCommand::SetTempo(_tempo) => {
                // Tempo will influence scheduling and clip triggering.
            }
            EngineCommand::SetTransport(state) => self.set_transport(state),
            EngineCommand::ReplaceGraph(graph) => self.replace_graph(graph)?,
            EngineCommand::SubmitMidi(events) => {
                let mut pending = self.pending_midi.lock();
                pending.extend(events);
            }
            EngineCommand::PlaySoundTest(clip) => {
                self.sound_test = Some(ClipPlayback::new(clip));
            }
        }
        Ok(())
    }

    pub fn process_block(&mut self, output: &mut AudioBuffer) -> anyhow::Result<()> {
        self.render_block_with(|master, _| {
            if output.channel_count() != master.channel_count() || output.len() != master.len() {
                output.resize(master.channel_count(), master.len());
            }
            for (target_channel, source_channel) in output.channels_mut().zip(master.channels()) {
                target_channel.copy_from_slice(source_channel);
            }
        })
    }

    pub(crate) fn render_block_with<R, F>(&mut self, mut visitor: F) -> anyhow::Result<R>
    where
        F: FnMut(&AudioBuffer, &[AudioBuffer]) -> R,
    {
        let start = Instant::now();
        let period_ns = self.block_period_ns;
        let block_start = self.automation_cursor;
        let block_len = self.config.block_size as u32;

        let result = (|| -> anyhow::Result<R> {
            self.drain_command_queue()?;
            let pending_midi = {
                let mut queue = self.pending_midi.lock();
                if queue.is_empty() {
                    Vec::new()
                } else {
                    std::mem::take(&mut *queue)
                }
            };
            let graph = match self.graph.read().clone() {
                Some(graph) => graph,
                None => {
                    let mut master = self.master_buffer.lock();
                    master.clear();
                    return Ok(visitor(&master, &[]));
                }
            };

            let plugin_ids = graph.plugin_ids();
            if plugin_ids.is_empty() {
                let mut master = self.master_buffer.lock();
                master.clear();
                return Ok(visitor(&master, &[]));
            }

            let processors_guard = self.processors.read();
            let processor_handles: Vec<_> = plugin_ids
                .iter()
                .map(|plugin_id| {
                    processors_guard.get(plugin_id).cloned().ok_or_else(|| {
                        anyhow::anyhow!("Missing processor for plugin ID: {:?}", plugin_id)
                    })
                })
                .collect::<anyhow::Result<_>>()?;
            drop(processors_guard);

            let latencies_guard = self.latencies.read();
            let latencies: Vec<usize> = plugin_ids
                .iter()
                .map(|plugin_id| *latencies_guard.get(plugin_id).unwrap_or(&0))
                .collect();
            drop(latencies_guard);

            let max_latency = latencies.iter().copied().max().unwrap_or(0);
            let automation_by_index =
                self.automation_events_for_block(&plugin_ids, block_start, block_len);

            let scratch_len = processor_handles.len();
            self.ensure_scratch_buffers(scratch_len);

            {
                let scratch_buffers = &mut self.scratch_buffers[..scratch_len];
                scratch_buffers.iter_mut().for_each(|buffer| buffer.clear());

                scratch_buffers.par_iter_mut().enumerate().try_for_each(
                    |(index, buffer)| -> anyhow::Result<()> {
                        let processor_handle = &processor_handles[index];
                        let mut processor = processor_handle.lock();

                        if let Some(events) = automation_by_index.get(index) {
                            for event in events {
                                processor.handle_automation_event(
                                    event.parameter,
                                    event.value,
                                    event.sample_offset as usize,
                                )?;
                            }
                        }

                        if !pending_midi.is_empty() {
                            processor.process_midi(&pending_midi)?;
                        }
                        let _guard = RtAllocGuard::enter();
                        processor.process(buffer)?;
                        Ok(())
                    },
                )?;
            }

            {
                self.apply_delay_compensation(&plugin_ids, &latencies, scratch_len, max_latency);
            }

            let result = {
                let scratch_buffers = &self.scratch_buffers[..scratch_len];
                let mut master = self.master_buffer.lock();
                graph::mixdown(&graph, &mut master, scratch_buffers);
                let _guard = RtAllocGuard::enter();
                self.tone_shaper.process(&mut master);

                if let Some(player) = self.sound_test.as_mut() {
                    if player.mix_into(&mut master) {
                        self.sound_test = None;
                    }
                }

                visitor(&master, scratch_buffers)
            };

            Ok(result)
        })();

        let elapsed = start.elapsed();
        self.metrics.record_block(elapsed, period_ns);
        self.emit_rt_metrics(elapsed, period_ns);
        if matches!(
            self.transport(),
            TransportState::Playing | TransportState::Recording
        ) {
            let rendered = self.config.block_size as u64;
            self.transport_metrics
                .sample_pos
                .fetch_add(rendered, Ordering::Relaxed);
            self.automation_cursor = self.automation_cursor.saturating_add(rendered);
        }
        result
    }

    fn emit_rt_metrics(&mut self, elapsed: Duration, period_ns: u64) {
        let Some(bridge) = self.rt_bridge.as_mut() else {
            return;
        };

        let snapshot = self.metrics.snapshot();

        if snapshot.xruns != self.last_reported_xruns {
            let count = snapshot.xruns.min(u64::from(u32::MAX)) as u32;
            bridge.push(RtEvent::Xrun { count });
            self.last_reported_xruns = snapshot.xruns;
        }

        if period_ns > 0 {
            let block_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
            let pct =
                ((u128::from(block_ns) * 1000) / u128::from(period_ns.max(1))).min(1000) as u16;
            if pct != self.last_reported_engine_load {
                bridge.push(RtEvent::EngineLoad { pct });
                self.last_reported_engine_load = pct;
            }
        }

        let max_block_us = (snapshot.max_block_ns / 1_000).min(u64::from(u32::MAX)) as u32;
        if max_block_us != 0 && max_block_us > self.last_reported_max_block_us {
            bridge.push(RtEvent::MaxBlockMicros { us: max_block_us });
            self.last_reported_max_block_us = max_block_us;
        }
    }

    fn drain_command_queue(&mut self) -> anyhow::Result<()> {
        while let Some(command) = self.command_queue.pop() {
            self.handle_command(command)?;
        }
        Ok(())
    }

    fn ensure_scratch_buffers(&mut self, len: usize) {
        if self.scratch_buffers.len() >= len {
            return;
        }

        for _ in self.scratch_buffers.len()..len {
            self.scratch_buffers
                .push(AudioBuffer::from_config(&self.config));
        }
    }

    fn automation_events_for_block(
        &mut self,
        plugin_ids: &[PluginId],
        block_start: u64,
        block_len: u32,
    ) -> Vec<Vec<AutomationEvent>> {
        let mut buckets = vec![Vec::new(); plugin_ids.len()];
        if plugin_ids.is_empty() || block_len == 0 {
            return buckets;
        }

        let mut lanes = self.automations.write();
        for (index, plugin_id) in plugin_ids.iter().enumerate() {
            if let Some(lane) = lanes.get_mut(plugin_id) {
                lane.render(block_start, block_len, &mut buckets[index]);
            }
        }

        buckets
    }

    fn apply_delay_compensation(
        &mut self,
        plugin_ids: &[PluginId],
        latencies: &[usize],
        scratch_len: usize,
        max_latency: usize,
    ) {
        if max_latency == 0 {
            for plugin_id in plugin_ids {
                if let Some(delay) = self.delay_lines.get_mut(plugin_id) {
                    if delay.delay_samples() != 0 {
                        delay.reset();
                    }
                }
            }
            return;
        }

        let buffers = &mut self.scratch_buffers[..scratch_len];

        for (index, plugin_id) in plugin_ids.iter().enumerate() {
            let plugin_latency = latencies.get(index).copied().unwrap_or(0);
            let additional_delay = max_latency.saturating_sub(plugin_latency);
            if additional_delay == 0 {
                if let Some(delay) = self.delay_lines.get_mut(plugin_id) {
                    if delay.delay_samples() != 0 {
                        delay.reset();
                    }
                }
                continue;
            }

            let channels = self.config.layout.channels() as usize;
            let delay = self
                .delay_lines
                .entry(*plugin_id)
                .or_insert_with(DelayCompensator::new);
            delay.configure(channels, additional_delay, self.config.block_size);
            delay.process(&mut buffers[index]);
        }
    }
}

struct ClipPlayback {
    clip: AudioClip,
    position: usize,
}

impl ClipPlayback {
    fn new(clip: AudioClip) -> Self {
        Self { clip, position: 0 }
    }

    fn mix_into(&mut self, buffer: &mut AudioBuffer) -> bool {
        let total_frames = self.clip.frames();
        if total_frames == 0 {
            return true;
        }

        let clip_channels = self.clip.channels();
        if clip_channels == 0 {
            return true;
        }

        let clip_samples = self.clip.samples();
        let mut position = self.position;
        let channel_count = buffer.channel_count();
        if channel_count == 0 {
            return true;
        }

        let available_frames = buffer.len();
        if available_frames == 0 {
            return true;
        }

        let data = buffer.as_mut_slice();

        for frame_index in 0..available_frames {
            if position >= total_frames {
                break;
            }

            for channel_index in 0..channel_count {
                let source_channel = channel_index.min(clip_channels - 1);
                if let Some(value) = clip_samples
                    .get(source_channel)
                    .and_then(|channel| channel.get(position))
                {
                    let dest_index = channel_index * available_frames + frame_index;
                    if dest_index < data.len() {
                        data[dest_index] += *value;
                    }
                }
            }

            position += 1;
        }

        self.position = position;
        self.position >= total_frames
    }
}

struct DelayCompensator {
    buffers: Vec<Vec<f32>>,
    write_positions: Vec<usize>,
    delay_samples: usize,
    capacity: usize,
    block_size: usize,
}

impl DelayCompensator {
    fn new() -> Self {
        Self {
            buffers: Vec::new(),
            write_positions: Vec::new(),
            delay_samples: 0,
            capacity: 0,
            block_size: 0,
        }
    }

    fn configure(&mut self, channels: usize, delay_samples: usize, block_size: usize) {
        let block_size = block_size.max(1);
        let capacity = delay_samples + block_size;

        if self.buffers.len() != channels {
            self.buffers = vec![vec![0.0; capacity]; channels];
            self.write_positions = vec![0; channels];
        } else if self.capacity != capacity {
            for buffer in &mut self.buffers {
                buffer.resize(capacity, 0.0);
            }
            for position in &mut self.write_positions {
                *position = 0;
            }
        }

        if self.delay_samples != delay_samples || self.block_size != block_size {
            for buffer in &mut self.buffers {
                buffer.fill(0.0);
            }
            for position in &mut self.write_positions {
                *position = 0;
            }
        }

        self.delay_samples = delay_samples;
        self.capacity = capacity;
        self.block_size = block_size;
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if self.delay_samples == 0 || self.capacity == 0 {
            return;
        }

        let capacity = self.capacity;
        let delay = self.delay_samples.min(capacity - 1);

        for (channel_index, channel) in buffer.channels_mut().enumerate() {
            if channel_index >= self.buffers.len() {
                break;
            }
            let storage = &mut self.buffers[channel_index];
            if storage.len() != capacity {
                continue;
            }

            let mut write_pos = self.write_positions[channel_index] % capacity;
            let mut read_pos = if write_pos >= delay {
                write_pos - delay
            } else {
                write_pos + capacity - delay
            };

            for sample in channel.iter_mut() {
                let delayed = storage[read_pos];
                storage[write_pos] = *sample;
                *sample = delayed;

                write_pos += 1;
                if write_pos == capacity {
                    write_pos = 0;
                }

                read_pos += 1;
                if read_pos == capacity {
                    read_pos = 0;
                }
            }
            self.write_positions[channel_index] = write_pos;
        }
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        for position in &mut self.write_positions {
            *position = 0;
        }
        self.delay_samples = 0;
    }

    fn delay_samples(&self) -> usize {
        self.delay_samples
    }
}

impl HarmoniqEngine {
    pub fn automation_sender(&self, plugin_id: PluginId) -> Option<AutomationSender> {
        self.automations
            .read()
            .get(&plugin_id)
            .map(|lane| lane.sender())
    }

    pub fn register_automation_parameter(
        &self,
        plugin_id: PluginId,
        spec: ParameterSpec,
    ) -> anyhow::Result<()> {
        let mut lanes = self.automations.write();
        let lane = lanes
            .get_mut(&plugin_id)
            .ok_or_else(|| anyhow::anyhow!("missing automation lane for plugin"))?;
        lane.register_parameter(spec);
        Ok(())
    }

    pub fn automation_parameter_index(&self, plugin_id: PluginId, name: &str) -> Option<usize> {
        self.automations
            .read()
            .get(&plugin_id)
            .and_then(|lane| lane.parameter_index_by_name(name))
    }

    pub fn automation_parameter_spec(
        &self,
        plugin_id: PluginId,
        parameter: usize,
    ) -> Option<ParameterSpec> {
        self.automations
            .read()
            .get(&plugin_id)
            .and_then(|lane| lane.parameter_spec(parameter))
    }
}

pub struct Engine {
    pub graph: crate::sched::graph::Graph,
    pub event_lane: crate::sched::events::EventLane,
    pub sample_pos: u64,
    pub transport: crate::transport::Transport,
    pub pool: crate::sched::executor::RtPool,
    pub parallel_cfg: crate::config::RtParallelCfg,
    max_nodes: usize,
    event_capacity: usize,
}

impl Engine {
    pub fn new(sr: u32, max_block: u32, event_capacity: usize) -> Self {
        let mut graph = crate::sched::graph::Graph::new();
        let pass_id = 0u32;
        let gain_id = 1u32;
        graph
            .nodes
            .push(Box::new(crate::sched::PassThrough::new(pass_id, "input")));
        graph
            .nodes
            .push(Box::new(crate::sched::Gain::new(gain_id, 0)));
        graph.topo = vec![pass_id, gain_id];
        crate::sched::graph::build(&mut graph);
        for node in graph.nodes.iter_mut() {
            node.prepare(sr, max_block);
        }

        let capacity = event_capacity.max(1);
        let parallel_cfg = crate::config::RtParallelCfg::default();
        let (_, worker_cores) = crate::rt::cpu::pick_cores(&parallel_cfg);
        let pool = crate::sched::executor::RtPool::new(
            graph.nodes.len().max(64),
            parallel_cfg.workers as usize,
            &worker_cores,
        );

        let max_nodes = graph.nodes.len();
        Self {
            graph,
            event_lane: crate::sched::events::EventLane::with_capacity(capacity),
            sample_pos: 0,
            transport: crate::transport::Transport::with_sample_rate(sr),
            pool,
            parallel_cfg,
            max_nodes,
            event_capacity: capacity,
        }
    }

    pub fn configure(&mut self, sr: u32, max_block: u32) {
        self.transport.set_sample_rate(sr);
        for node in self.graph.nodes.iter_mut() {
            node.prepare(sr, max_block);
        }
        self.rebuild();
    }

    pub fn reset(&mut self) {
        self.sample_pos = 0;
        self.transport.sample_pos.store(0, Ordering::Relaxed);
        self.event_lane = crate::sched::events::EventLane::with_capacity(self.event_capacity);
    }

    pub fn rebuild(&mut self) {
        crate::sched::graph::build(&mut self.graph);
        if self.graph.nodes.len() > self.max_nodes
            || self.pool.capacity() < self.graph.nodes.len()
            || self.pool_capacity_mismatch()
        {
            let (_, worker_cores) = crate::rt::cpu::pick_cores(&self.parallel_cfg);
            self.pool = crate::sched::executor::RtPool::new(
                self.graph.nodes.len().max(64),
                self.parallel_cfg.workers as usize,
                &worker_cores,
            );
        }
        self.max_nodes = self.graph.nodes.len();
    }

    fn pool_capacity_mismatch(&self) -> bool {
        self.pool.worker_count() != self.parallel_cfg.workers as usize
    }
}
