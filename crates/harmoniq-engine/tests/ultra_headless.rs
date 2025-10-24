use harmoniq_engine::{AudioBuffer, BufferConfig, ChannelLayout, HarmoniqEngine};

#[test]
fn render_default_graph_without_xruns() {
    // Use a moderate block size so that the target block period is generous
    // enough for CI environments while still verifying that processing stays
    // well below half of the available time slice.
    let config = BufferConfig::new(48_000.0, 256, ChannelLayout::Stereo);
    let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");
    let metrics = engine.metrics_collector();
    let mut buffer = AudioBuffer::from_config(config.clone());

    for _ in 0..8 {
        engine.process_block(&mut buffer).expect("warm-up block");
    }
    metrics.reset();

    let total_blocks = ((config.sample_rate as usize) * 5) / config.block_size.max(1);
    for _ in 0..total_blocks {
        engine.process_block(&mut buffer).expect("process");
    }

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.xruns, 0, "xruns should remain at zero");

    let block_period_ns = ((config.block_size as f64 / config.sample_rate as f64) * 1e9) as u64;
    let half_period_ns = (block_period_ns / 2).max(1);
    assert!(
        snapshot.max_block_ns < half_period_ns,
        "max block time {}ns exceeded half period {}ns",
        snapshot.max_block_ns,
        half_period_ns
    );
}
