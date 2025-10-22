#[cfg(feature = "cpal")]
use anyhow::Result;
#[cfg(feature = "cpal")]
use dsp::{OnePoleLowPass, SineOscillator};
#[cfg(feature = "cpal")]
use engine_graph::GraphBuilder;
#[cfg(feature = "cpal")]
use engine_rt::transport::TransportCommand;
#[cfg(feature = "cpal")]
use io_backends::cpal_backend::CpalBackend;
#[cfg(feature = "cpal")]
use io_backends::AudioBackend;
#[cfg(feature = "cpal")]
use server::{EngineServer, ServerConfig};
#[cfg(feature = "cpal")]
use std::sync::Arc;
#[cfg(feature = "cpal")]
use std::thread;
#[cfg(feature = "cpal")]
use std::time::Duration;

#[cfg(feature = "cpal")]
fn build_graph() -> Result<engine_graph::AudioGraph> {
    let mut builder = GraphBuilder::new();
    let osc = builder.add_node(Box::new(SineOscillator::new(330.0, 0.25)), 0, 1);
    let filter = builder.add_node(Box::new(OnePoleLowPass::new(900.0)), 1, 1);
    builder.connect(osc, 0, filter, 0)?;
    builder.designate_output(filter);
    builder.build()
}

#[cfg(feature = "cpal")]
fn main() -> Result<()> {
    let backend: Arc<dyn AudioBackend> = Arc::new(CpalBackend::new());
    let mut server = EngineServer::new(ServerConfig::default(), backend);
    let graph = build_graph()?;
    server.engine_mut().configure_graph(graph)?;
    if let Err(err) = server.engine_mut().start() {
        eprintln!("Failed to start audio engine: {err}");
        return Ok(());
    }

    let client_a = server.connect_client();
    let client_b = server.connect_client();

    client_a.send_transport(TransportCommand::Play)?;
    thread::sleep(Duration::from_millis(500));
    client_b.send_transport(TransportCommand::SetPosition { samples: 0 })?;
    thread::sleep(Duration::from_secs(1));
    server.engine_mut().stop()?;
    Ok(())
}

#[cfg(not(feature = "cpal"))]
fn main() {
    println!(
        "multi-client-server built without the `cpal` feature. Enable it to run the realtime demo."
    );
}
