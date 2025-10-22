use engine_core::{EngineConfig, LatencyMetrics};
use io_backends::StreamConfig;

#[test]
fn latency_computation() {
    let config = EngineConfig::default().with_stream(StreamConfig {
        sample_rate: 48_000,
        channels: 2,
        block_size: 128,
    });
    let latency = LatencyMetrics::new(config.stream.sample_rate, config.stream.block_size, 32);
    assert!(latency.round_trip_ms > 0.0);
}
