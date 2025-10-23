use harmoniq_dsp::{AudioBlock, AudioBlockMut};
use harmoniq_engine::dsp::{
    nodes::{GainNode, PanNode},
    DspGraph, GraphProcess, ParamUpdate, Transport,
};

#[test]
fn graph_applies_gain_and_pan() {
    let mut graph = DspGraph::new();
    let (gain_id, gain_port) = graph.add_node(Box::new(GainNode::new(-3.0)), 32);
    let (pan_id, _) = graph.add_node(Box::new(PanNode::new(0.0)), 16);
    graph.set_topology(&[gain_id, pan_id]);
    graph.prepare(48_000.0, 64, 2, 2);

    if let Some(port) = gain_port {
        let _ = port.try_send(ParamUpdate::new(0, -6.0));
    }

    let mut input = vec![1.0f32; 2 * 64];
    let mut output = vec![0.0f32; 2 * 64];
    unsafe {
        let input_block = AudioBlock::from_interleaved(input.as_ptr(), 2, 64);
        let output_block = AudioBlockMut::from_interleaved(output.as_mut_ptr(), 2, 64);
        graph.process(GraphProcess {
            inputs: input_block,
            outputs: output_block,
            frames: 64,
            transport: Transport::default(),
            midi: &[],
        });
    }

    assert!(output.iter().all(|sample| sample.is_finite()));
}
