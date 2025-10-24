use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use harmoniq_engine::{
    nodes::NodeOsc, AudioBuffer, BufferConfig, ChannelLayout, GraphBuilder, HarmoniqEngine,
};

fn main() -> anyhow::Result<()> {
    let duration_secs = std::env::var("SOAK_DURATION_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(8 * 60 * 60);
    let report_secs = std::env::var("SOAK_REPORT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);
    let log_path = std::env::var("SOAK_LOG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("soak_xruns.log"));

    let config = BufferConfig::new(48_000.0, 256, ChannelLayout::Stereo);
    let mut engine = HarmoniqEngine::new(config.clone())?;
    let mut builder = GraphBuilder::new();
    let osc = engine.register_processor(Box::new(NodeOsc::new(440.0).with_amplitude(0.2)))?;
    let node = builder.add_node(osc);
    builder.connect_to_mixer(node, 0.8)?;
    engine.replace_graph(builder.build())?;

    let metrics = engine.metrics_collector();
    metrics.reset();

    let mut buffer = AudioBuffer::from_config(&config);
    for _ in 0..16 {
        engine.process_block(&mut buffer)?;
    }
    metrics.reset();

    let mut log = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)?;
    writeln!(log, "elapsed_s,blocks,xruns,last_block_ns,max_block_ns")?;

    let block_period =
        Duration::from_secs_f64((config.block_size as f64) / (config.sample_rate as f64));
    let report_interval = Duration::from_secs(report_secs.max(1));

    let start = Instant::now();
    let mut last_report = start;
    let mut blocks_processed: u64 = 0;
    let mut last_snapshot = metrics.snapshot();

    while start.elapsed() < Duration::from_secs(duration_secs) {
        let block_start = Instant::now();
        engine.process_block(&mut buffer)?;
        blocks_processed += 1;

        if let Some(remaining) = block_period.checked_sub(block_start.elapsed()) {
            std::thread::sleep(remaining);
        }

        if last_report.elapsed() >= report_interval {
            let snapshot = metrics.snapshot();
            writeln!(
                log,
                "{:.3},{},{},{},{}",
                start.elapsed().as_secs_f64(),
                blocks_processed,
                snapshot.xruns,
                snapshot.last_block_ns,
                snapshot.max_block_ns,
            )?;
            log.flush()?;

            if snapshot.xruns > last_snapshot.xruns {
                eprintln!(
                    "[soak] xruns increased to {} at {:.1}s",
                    snapshot.xruns,
                    start.elapsed().as_secs_f32()
                );
            }

            last_report = Instant::now();
            last_snapshot = snapshot;
        }
    }

    let final_snapshot = metrics.snapshot();
    writeln!(
        log,
        "{:.3},{},{},{},{}",
        start.elapsed().as_secs_f64(),
        blocks_processed,
        final_snapshot.xruns,
        final_snapshot.last_block_ns,
        final_snapshot.max_block_ns,
    )?;
    log.flush()?;

    println!(
        "render soak complete: duration={:.1}s blocks={} xruns={}",
        start.elapsed().as_secs_f32(),
        blocks_processed,
        final_snapshot.xruns
    );

    Ok(())
}
