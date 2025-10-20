use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam::queue::ArrayQueue;
use parking_lot::{Mutex, RwLock};

use crate::{
    graph::{self, GraphHandle},
    plugin::PluginId,
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
}

/// Central Harmoniq engine responsible for orchestrating the processing graph.
pub struct HarmoniqEngine {
    config: BufferConfig,
    processors: RwLock<HashMap<PluginId, Box<dyn AudioProcessor>>>,
    graph: RwLock<Option<GraphHandle>>,
    master_buffer: Mutex<AudioBuffer>,
    next_plugin_id: AtomicU64,
    transport: RwLock<TransportState>,
    command_queue: Arc<ArrayQueue<EngineCommand>>,
}

impl HarmoniqEngine {
    pub fn new(config: BufferConfig) -> anyhow::Result<Self> {
        let command_queue = Arc::new(ArrayQueue::new(COMMAND_QUEUE_CAPACITY));
        Ok(Self {
            master_buffer: Mutex::new(AudioBuffer::from_config(config.clone())),
            processors: RwLock::new(HashMap::new()),
            graph: RwLock::new(None),
            next_plugin_id: AtomicU64::new(1),
            transport: RwLock::new(TransportState::Stopped),
            command_queue,
            config,
        })
    }

    pub fn config(&self) -> &BufferConfig {
        &self.config
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
        self.processors.write().insert(id, processor);
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
        }
        Ok(())
    }

    pub fn process_block(&mut self, output: &mut AudioBuffer) -> anyhow::Result<()> {
        self.drain_command_queue()?;
        let graph = match self.graph.read().clone() {
            Some(graph) => graph,
            None => {
                output.clear();
                return Ok(());
            }
        };

        let mut processors = self.processors.write();
        let mut scratch_buffers: Vec<AudioBuffer> = Vec::with_capacity(graph.nodes.len());
        for plugin_id in &graph.nodes {
            let Some(processor) = processors.get_mut(plugin_id) else {
                anyhow::bail!("Missing processor for plugin ID: {:?}", plugin_id);
            };
            let mut buffer = AudioBuffer::from_config(self.config.clone());
            processor.process(&mut buffer)?;
            scratch_buffers.push(buffer);
        }

        {
            let mut master = self.master_buffer.lock();
            graph::mixdown(&mut master, &scratch_buffers, &graph.mixer_gains);
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
