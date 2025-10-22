use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam::queue::ArrayQueue;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

use crate::{
    graph::{self, GraphHandle},
    plugin::{MidiEvent, PluginId},
    tone::ToneShaper,
    AudioBuffer, AudioProcessor, BufferConfig,
};

const COMMAND_QUEUE_CAPACITY: usize = 1024;

/// Transport state shared with UI and sequencing components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Playing,
    Recording,
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
    command_queue: Arc<ArrayQueue<EngineCommand>>,
    pending_midi: Mutex<Vec<MidiEvent>>,
    automations: RwLock<HashMap<PluginId, AutomationLane>>,
    latencies: RwLock<HashMap<PluginId, usize>>,
    delay_lines: HashMap<PluginId, DelayCompensator>,
    scratch_buffers: Vec<AudioBuffer>,
}

impl HarmoniqEngine {
    pub fn new(config: BufferConfig) -> anyhow::Result<Self> {
        let command_queue = Arc::new(ArrayQueue::new(COMMAND_QUEUE_CAPACITY));
        let tone_shaper = ToneShaper::new(&config);

        Ok(Self {
            master_buffer: Mutex::new(AudioBuffer::from_config(config.clone())),
            processors: RwLock::new(HashMap::new()),
            graph: RwLock::new(None),
            next_plugin_id: AtomicU64::new(1),
            transport: RwLock::new(TransportState::Stopped),
            command_queue,
            pending_midi: Mutex::new(Vec::new()),
            config,
            tone_shaper,
            automations: RwLock::new(HashMap::new()),
            latencies: RwLock::new(HashMap::new()),
            delay_lines: HashMap::new(),
            scratch_buffers: Vec::new(),
        })
    }

    pub fn config(&self) -> &BufferConfig {
        &self.config
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

    pub fn set_transport(&self, state: TransportState) {
        *self.transport.write() = state;
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
        let ring = HeapRb::new(256);
        let (producer, consumer) = ring.split();
        lanes.insert(id, AutomationLane::new(producer, consumer));
        Ok(id)
    }

    pub fn replace_graph(&self, graph: GraphHandle) -> anyhow::Result<()> {
        if graph.is_empty() {
            anyhow::bail!("graph must contain at least one node");
        }
        *self.graph.write() = Some(graph);
        Ok(())
    }

    pub fn execute_command(&self, command: EngineCommand) -> anyhow::Result<()> {
        self.handle_command(command)
    }

    fn handle_command(&self, command: EngineCommand) -> anyhow::Result<()> {
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
        }
        Ok(())
    }

    pub fn process_block(&mut self, output: &mut AudioBuffer) -> anyhow::Result<()> {
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
                output.clear();
                return Ok(());
            }
        };

        let plugin_ids = graph.plugin_ids();
        if plugin_ids.is_empty() {
            output.clear();
            return Ok(());
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
        let automation_by_index = self.automation_events_for_block(&plugin_ids);

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
                    processor.process(buffer)?;
                    Ok(())
                },
            )?;
        }

        {
            self.apply_delay_compensation(&plugin_ids, &latencies, scratch_len, max_latency);
        }

        {
            let scratch_buffers = &self.scratch_buffers[..scratch_len];
            let mut master = self.master_buffer.lock();
            graph::mixdown(&graph, &mut master, scratch_buffers);
            self.tone_shaper.process(&mut master);
            for (target_channel, source_channel) in output.channels_mut().zip(master.channels()) {
                target_channel.copy_from_slice(source_channel);
            }
        }

        Ok(())
    }

    fn drain_command_queue(&self) -> anyhow::Result<()> {
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
                .push(AudioBuffer::from_config(self.config.clone()));
        }
    }

    fn automation_events_for_block(
        &mut self,
        plugin_ids: &[PluginId],
    ) -> Vec<Vec<AutomationEvent>> {
        if plugin_ids.is_empty() {
            return Vec::new();
        }

        let mut buckets = vec![Vec::new(); plugin_ids.len()];
        let mut index_map = HashMap::new();
        for (index, plugin_id) in plugin_ids.iter().enumerate() {
            index_map.insert(*plugin_id, index);
        }

        for mut event in self.drain_automation_events() {
            if let Some(&index) = index_map.get(&event.plugin_id) {
                if self.config.block_size > 0 {
                    let max_offset = (self.config.block_size - 1) as u32;
                    if event.sample_offset > max_offset {
                        event.sample_offset = max_offset;
                    }
                } else {
                    event.sample_offset = 0;
                }
                buckets[index].push(event);
            }
        }

        for bucket in &mut buckets {
            bucket.sort_by_key(|event| event.sample_offset);
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

#[derive(Debug, Clone)]
pub struct AutomationEvent {
    pub plugin_id: PluginId,
    pub parameter: usize,
    pub value: f32,
    pub sample_offset: u32,
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

#[derive(Clone)]
pub struct AutomationSender {
    producer: Arc<Mutex<HeapProducer<AutomationEvent>>>,
}

impl AutomationSender {
    pub fn send(&self, event: AutomationEvent) -> Result<(), AutomationEvent> {
        let mut producer = self.producer.lock();
        producer.push(event).map_err(|event| event)
    }
}

struct AutomationLane {
    producer: Arc<Mutex<HeapProducer<AutomationEvent>>>,
    consumer: HeapConsumer<AutomationEvent>,
}

impl AutomationLane {
    fn new(
        producer: HeapProducer<AutomationEvent>,
        consumer: HeapConsumer<AutomationEvent>,
    ) -> Self {
        Self {
            producer: Arc::new(Mutex::new(producer)),
            consumer,
        }
    }

    fn sender(&self) -> AutomationSender {
        AutomationSender {
            producer: Arc::clone(&self.producer),
        }
    }
}

impl HarmoniqEngine {
    pub fn automation_sender(&self, plugin_id: PluginId) -> Option<AutomationSender> {
        self.automations
            .read()
            .get(&plugin_id)
            .map(|lane| lane.sender())
    }

    fn drain_automation_events(&self) -> Vec<AutomationEvent> {
        let mut events = Vec::new();
        let mut lanes = self.automations.write();
        for (plugin_id, lane) in lanes.iter_mut() {
            while let Some(mut event) = lane.consumer.pop() {
                if event.plugin_id.0 == 0 {
                    event.plugin_id = *plugin_id;
                }
                events.push(event);
            }
        }
        events
    }
}
