use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam::queue::ArrayQueue;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use ringbuf::{Consumer, HeapRb, Producer};

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
        let id = PluginId(self.next_plugin_id.fetch_add(1, Ordering::SeqCst));
        let descriptor = processor.descriptor();
        tracing::info!("Registered processor: {}", descriptor);
        let shared = Arc::new(Mutex::new(processor));
        self.processors.write().insert(id, shared);
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

        let processors_guard = self.processors.read();
        let plugin_ids = graph.plugin_ids();
        let processor_handles: Vec<_> = plugin_ids
            .iter()
            .map(|plugin_id| {
                processors_guard.get(plugin_id).cloned().ok_or_else(|| {
                    anyhow::anyhow!("Missing processor for plugin ID: {:?}", plugin_id)
                })
            })
            .collect::<anyhow::Result<_>>()?;
        drop(processors_guard);

        let mut scratch_buffers: Vec<AudioBuffer> = (0..processor_handles.len())
            .map(|_| AudioBuffer::from_config(self.config.clone()))
            .collect();

        let _automation_events = self.drain_automation_events();

        scratch_buffers
            .par_iter_mut()
            .zip(processor_handles.par_iter())
            .try_for_each(|(buffer, processor)| -> anyhow::Result<()> {
                let mut processor = processor.lock();
                if !pending_midi.is_empty() {
                    processor.process_midi(&pending_midi)?;
                }
                processor.process(buffer)?;
                Ok(())
            })?;

        {
            let mut master = self.master_buffer.lock();
            graph::mixdown(&graph, &mut master, &scratch_buffers);
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
}

#[derive(Debug, Clone)]
pub struct AutomationEvent {
    pub plugin_id: PluginId,
    pub parameter: usize,
    pub value: f32,
}

#[derive(Clone)]
pub struct AutomationSender {
    producer: Arc<Mutex<Producer<AutomationEvent>>>,
}

impl AutomationSender {
    pub fn send(&self, event: AutomationEvent) -> Result<(), AutomationEvent> {
        let mut producer = self.producer.lock();
        producer.push(event).map_err(|event| event)
    }
}

struct AutomationLane {
    producer: Arc<Mutex<Producer<AutomationEvent>>>,
    consumer: Consumer<AutomationEvent>,
}

impl AutomationLane {
    fn new(producer: Producer<AutomationEvent>, consumer: Consumer<AutomationEvent>) -> Self {
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
