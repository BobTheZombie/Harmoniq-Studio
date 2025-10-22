use anyhow::Result;
use engine_core::{Engine, EngineConfig, RealTimeScheduler};
use engine_rt::transport::TransportCommand;
use io_backends::AudioBackend;
use midi::{MidiEvent, MidiPort};
use std::sync::Arc;

#[derive(Clone)]
pub struct ServerConfig {
    pub engine: EngineConfig,
    pub max_clients: usize,
    pub midi_queue_capacity: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            max_clients: 4,
            midi_queue_capacity: 256,
        }
    }
}

pub struct EngineServer {
    engine: Engine,
    scheduler: RealTimeScheduler,
    clients: Vec<ClientHandle>,
    next_client_id: usize,
    midi_port: MidiPort,
    max_clients: usize,
}

impl EngineServer {
    pub fn new(config: ServerConfig, backend: Arc<dyn AudioBackend>) -> Self {
        let engine = Engine::new(config.engine.clone(), backend);
        let scheduler = engine.scheduler();
        Self {
            engine,
            scheduler,
            clients: Vec::new(),
            next_client_id: 1,
            midi_port: MidiPort::new(config.midi_queue_capacity),
            max_clients: config.max_clients,
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn connect_client(&mut self) -> ClientHandle {
        if self.clients.len() >= self.max_clients {
            panic!("maximum number of clients reached");
        }
        let id = self.next_client_id;
        self.next_client_id += 1;
        let handle = ClientHandle {
            id,
            transport_queue: self.scheduler.transport_queue(),
        };
        self.clients.push(handle.clone());
        handle
    }

    pub fn broadcast_midi(&self, event: MidiEvent) {
        let _ = self.midi_port.push(event);
    }
}

#[derive(Clone)]
pub struct ClientHandle {
    pub id: usize,
    transport_queue: engine_rt::EventQueue<TransportCommand>,
}

impl ClientHandle {
    pub fn send_transport(&self, command: TransportCommand) -> Result<()> {
        self.transport_queue
            .try_push(command)
            .map_err(|err| anyhow::anyhow!("transport queue full: {err}"))
    }
}
