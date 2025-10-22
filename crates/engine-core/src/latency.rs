#[derive(Debug, Clone, Copy, Default)]
pub struct LatencyMetrics {
    pub round_trip_ms: f32,
    pub buffer_ms: f32,
    pub graph_latency_samples: usize,
}

impl LatencyMetrics {
    pub fn new(sample_rate: u32, block_size: usize, graph_latency_samples: usize) -> Self {
        let buffer_ms = block_size as f32 / sample_rate as f32 * 1000.0;
        let graph_ms = graph_latency_samples as f32 / sample_rate as f32 * 1000.0;
        Self {
            round_trip_ms: buffer_ms * 2.0 + graph_ms,
            buffer_ms,
            graph_latency_samples,
        }
    }
}
