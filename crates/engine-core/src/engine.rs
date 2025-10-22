use crate::config::EngineConfig;
use crate::latency::LatencyMetrics;
use crate::scheduler::RealTimeScheduler;
use anyhow::{anyhow, Result};
use engine_graph::{AudioGraph, GraphConfig, GraphExecutor};
use engine_rt::transport::{TransportCommand, TransportState};
use engine_rt::{AudioProcessor, CallbackHandle};
use io_backends::{AudioBackend, AudioStream, DeviceId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Engine {
    config: EngineConfig,
    backend: Arc<dyn AudioBackend>,
    scheduler: RealTimeScheduler,
    graph: Option<GraphExecutor>,
    stream: Option<Box<dyn AudioStream>>,
    callback: Option<CallbackHandle>,
    transport_clock: Arc<AtomicU64>,
    latency: LatencyMetrics,
}

impl Engine {
    pub fn new(config: EngineConfig, backend: Arc<dyn AudioBackend>) -> Self {
        let latency = LatencyMetrics::new(config.stream.sample_rate, config.stream.block_size, 0);
        Self {
            scheduler: RealTimeScheduler::new(config.transport_queue_capacity),
            config,
            backend,
            graph: None,
            stream: None,
            callback: None,
            transport_clock: Arc::new(AtomicU64::new(0)),
            latency,
        }
    }

    pub fn configure_graph(&mut self, graph: AudioGraph) -> Result<()> {
        let graph_config = GraphConfig::new(
            self.config.stream.sample_rate,
            self.config.stream.block_size,
            self.config.stream.channels,
        );
        let executor = graph.into_executor(graph_config)?;
        self.latency = LatencyMetrics::new(
            self.config.stream.sample_rate,
            self.config.stream.block_size,
            executor.latency_samples(),
        );
        self.graph = Some(executor);
        Ok(())
    }

    pub fn start(&mut self) -> Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }
        let executor = self
            .graph
            .take()
            .ok_or_else(|| anyhow!("audio graph not configured"))?;
        let renderer = EngineRenderer::new(
            executor,
            self.scheduler.transport_queue(),
            self.transport_clock.clone(),
        );
        let callback_handle = CallbackHandle::new(Box::new(renderer));
        let device_id = self
            .config
            .backend
            .clone()
            .map(DeviceId)
            .unwrap_or_else(|| DeviceId(String::new()));
        let stream = self
            .backend
            .open_output_stream(&device_id, &self.config.stream, callback_handle.clone())
            .map_err(|err| anyhow!("failed to open output stream: {err}"))?;
        stream
            .start()
            .map_err(|err| anyhow!("failed to start stream: {err}"))?;
        self.stream = Some(stream);
        self.callback = Some(callback_handle);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(stream) = &self.stream {
            stream
                .stop()
                .map_err(|err| anyhow!("failed to stop stream: {err}"))?;
        }
        self.stream = None;
        self.callback = None;
        self.transport_clock.store(0, Ordering::Relaxed);
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    pub fn latency(&self) -> LatencyMetrics {
        self.latency
    }

    pub fn transport_position_samples(&self) -> u64 {
        self.transport_clock.load(Ordering::Relaxed)
    }

    pub fn send_transport_command(&self, command: TransportCommand) -> Result<()> {
        self.scheduler
            .schedule_transport(command)
            .map_err(|err| anyhow!("failed to enqueue transport command: {err}"))
    }

    pub fn scheduler(&self) -> RealTimeScheduler {
        self.scheduler.clone()
    }
}

struct EngineRenderer {
    executor: GraphExecutor,
    transport: TransportState,
    transport_commands: engine_rt::EventQueue<TransportCommand>,
    transport_clock: Arc<AtomicU64>,
}

impl EngineRenderer {
    fn new(
        executor: GraphExecutor,
        transport_commands: engine_rt::EventQueue<TransportCommand>,
        transport_clock: Arc<AtomicU64>,
    ) -> Self {
        let transport = TransportState::new(executor.sample_rate());
        Self {
            executor,
            transport,
            transport_commands,
            transport_clock,
        }
    }
}

impl AudioProcessor for EngineRenderer {
    fn process(&mut self, buffer: &mut engine_rt::InterleavedAudioBuffer<'_>) {
        while let Ok(command) = self.transport_commands.try_pop() {
            self.transport.apply(command);
        }
        self.executor.process(buffer.outputs, &self.transport);
        self.transport.advance(buffer.frames as u64);
        self.transport_clock
            .store(self.transport.position_samples(), Ordering::Relaxed);
    }
}
