use dsp::{OnePoleLowPass, SineOscillator};
use engine_graph::{GraphBuilder, GraphConfig};

#[test]
fn graph_executes_without_panicking() {
    let mut builder = GraphBuilder::new();
    let osc = builder.add_node(Box::new(SineOscillator::new(110.0, 0.1)), 0, 1);
    let filter = builder.add_node(Box::new(OnePoleLowPass::new(800.0)), 1, 1);
    builder.connect(osc, 0, filter, 0).unwrap();
    builder.designate_output(filter);
    let graph = builder.build().unwrap();
    let mut executor = graph
        .into_executor(GraphConfig::new(48_000, 128, 2))
        .unwrap();
    let mut buffer = vec![0.0f32; 128 * 2];
    let transport = engine_rt::TransportState::new(48_000);
    executor.process(&mut buffer, &transport);
}
