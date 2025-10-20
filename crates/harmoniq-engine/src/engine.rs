use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::{Mutex, RwLock};

use crate::{
    graph::{self, GraphHandle},
    plugin::PluginId,
    AudioBuffer, AudioProcessor, BufferConfig,
};

/// Transport state shared with UI and sequencing components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Playing,
    Recording,
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
}

impl HarmoniqEngine {
    pub fn new(config: BufferConfig) -> anyhow::Result<Self> {
        Ok(Self {
            master_buffer: Mutex::new(AudioBuffer::from_config(config.clone())),
            processors: RwLock::new(HashMap::new()),
            graph: RwLock::new(None),
            next_plugin_id: AtomicU64::new(1),
            transport: RwLock::new(TransportState::Stopped),
            config,
        })
    }

    pub fn config(&self) -> &BufferConfig {
        &self.config
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
}
