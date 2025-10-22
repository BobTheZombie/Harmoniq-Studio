#[cfg(feature = "cpal")]
use anyhow::Result;
#[cfg(feature = "cpal")]
use dsp::{OnePoleLowPass, SineOscillator};
#[cfg(feature = "cpal")]
use engine_core::{Engine, EngineConfig};
#[cfg(feature = "cpal")]
use engine_graph::GraphBuilder;
#[cfg(feature = "cpal")]
use engine_rt::transport::TransportCommand;
#[cfg(feature = "cpal")]
use io_backends::cpal_backend::CpalBackend;
#[cfg(feature = "cpal")]
use io_backends::AudioBackend;
#[cfg(feature = "cpal")]
use std::sync::Arc;
#[cfg(feature = "cpal")]
use std::thread;
#[cfg(feature = "cpal")]
use std::time::Duration;

#[cfg(feature = "cpal")]
fn build_graph() -> Result<engine_graph::AudioGraph> {
    let mut builder = GraphBuilder::new();
    let osc = builder.add_node(Box::new(SineOscillator::new(220.0, 0.2)), 0, 1);
    let filter = builder.add_node(Box::new(OnePoleLowPass::new(1200.0)), 1, 1);
    builder.connect(osc, 0, filter, 0)?;
    builder.designate_output(filter);
    builder.build()
}

#[cfg(feature = "cpal")]
fn main() -> Result<()> {
    let backend: Arc<dyn AudioBackend> = Arc::new(CpalBackend::new());
    let mut engine = Engine::new(EngineConfig::default(), backend);
    let graph = build_graph()?;
    engine.configure_graph(graph)?;
    if let Err(err) = engine.start() {
        eprintln!("Failed to start audio engine: {err}");
        return Ok(());
    }
    engine.send_transport_command(TransportCommand::Play)?;
    println!("Harmoniq minimal host running. Playing a filtered sine for two seconds...");
    thread::sleep(Duration::from_secs(2));
    engine.stop()?;
    Ok(())
}

#[cfg(not(feature = "cpal"))]
fn main() {
    println!("minimal-host built without the `cpal` feature. Enable it to run the realtime demo.");
}
