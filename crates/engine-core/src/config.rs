use io_backends::StreamConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub stream: StreamConfig,
    pub backend: Option<String>,
    pub control_queue_capacity: usize,
    pub transport_queue_capacity: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            stream: StreamConfig::default(),
            backend: None,
            control_queue_capacity: 1024,
            transport_queue_capacity: 256,
        }
    }
}

impl EngineConfig {
    pub fn with_stream(mut self, stream: StreamConfig) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_backend<S: Into<String>>(mut self, backend: S) -> Self {
        self.backend = Some(backend.into());
        self
    }
}
