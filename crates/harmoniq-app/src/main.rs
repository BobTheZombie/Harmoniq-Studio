use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use harmoniq_engine::{
    BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine, TransportState,
};
use harmoniq_plugins::{GainPlugin, NoisePlugin, SineSynth};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(author, version, about = "Harmoniq Studio prototype CLI")]
struct Cli {
    /// Sample rate used for the offline rendering test
    #[arg(long, default_value_t = 48_000.0)]
    sample_rate: f32,

    /// Block size used for internal processing
    #[arg(long, default_value_t = 512)]
    block_size: usize,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .ok();

    let args = Cli::parse();
    let config = BufferConfig::new(args.sample_rate, args.block_size, ChannelLayout::Stereo);
    let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
    let command_queue = engine.command_queue();

    let sine = engine
        .register_processor(Box::new(SineSynth::with_frequency(220.0)))
        .context("register sine")?;
    let noise = engine
        .register_processor(Box::new(NoisePlugin))
        .context("register noise")?;
    let gain = engine
        .register_processor(Box::new(GainPlugin::new(0.4)))
        .context("register gain")?;

    let mut graph_builder = GraphBuilder::new();
    let sine_node = graph_builder.add_node(sine);
    graph_builder.connect_to_mixer(sine_node, 0.7)?;
    let noise_node = graph_builder.add_node(noise);
    graph_builder.connect_to_mixer(noise_node, 0.1)?;
    let gain_node = graph_builder.add_node(gain);
    graph_builder.connect_to_mixer(gain_node, 1.0)?;

    command_queue
        .try_send(EngineCommand::ReplaceGraph(graph_builder.build()))
        .map_err(|_| anyhow!("command queue full while replacing graph"))?;
    command_queue
        .try_send(EngineCommand::SetTransport(TransportState::Playing))
        .map_err(|_| anyhow!("command queue full while updating transport"))?;

    let mut buffer = harmoniq_engine::AudioBuffer::from_config(config.clone());
    for _ in 0..10 {
        engine.process_block(&mut buffer)?;
        std::thread::sleep(Duration::from_millis(10));
    }

    println!(
        "Rendered {} frames across {} channels",
        buffer.len(),
        config.layout.channels()
    );

    Ok(())
}
