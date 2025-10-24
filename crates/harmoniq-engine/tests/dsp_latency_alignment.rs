use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use harmoniq_dsp::{AudioBlock, AudioBlockMut};
use harmoniq_engine::dsp::{
    nodes::{GainNode, PanNode, StereoDelayNode, SvfLowpassNode},
    DspGraph, GraphProcess, ParamUpdate, Transport,
};

fn read_golden(path: PathBuf) -> Vec<f32> {
    let file = File::open(path).expect("failed to open golden");
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(Result::ok)
        .filter_map(|line| line.parse::<f32>().ok())
        .collect()
}

fn write_input(frames: usize, channels: usize) -> (Vec<f32>, Vec<f32>) {
    let mut input = vec![0.0f32; frames * channels];
    input[0] = 1.0;
    let output = vec![0.0f32; frames * channels];
    (input, output)
}

#[test]
fn dsp_graph_latency_alignment_matches_golden() {
    let frames = 64u32;
    let channels = 2u32;
    let mut graph = DspGraph::new();
    let (gain_id, gain_port) = graph.add_node(Box::new(GainNode::new(0.0)), 32);
    let (lp_id, _) = graph.add_node(Box::new(SvfLowpassNode::new(12_000.0, 0.7)), 16);
    let (delay_id, delay_port) = graph.add_node(Box::new(StereoDelayNode::new(48_000.0, 1.0)), 32);
    let (pan_id, _) = graph.add_node(Box::new(PanNode::new(0.0)), 16);
    graph.set_topology(&[gain_id, lp_id, delay_id, pan_id]);

    graph.prepare(48_000.0, frames, channels, channels);

    if let Some(port) = gain_port {
        let _ = port.try_send(ParamUpdate::new(0, -3.0));
    }
    if let Some(port) = delay_port {
        let _ = port.try_send(ParamUpdate::new(0, 0.001));
        let _ = port.try_send(ParamUpdate::new(1, 0.25));
        let _ = port.try_send(ParamUpdate::new(2, 0.5));
    }

    let (input, mut output) = write_input(frames as usize, channels as usize);

    unsafe {
        let input_block = AudioBlock::from_interleaved(input.as_ptr(), channels, frames);
        let output_block = AudioBlockMut::from_interleaved(output.as_mut_ptr(), channels, frames);
        graph.process(GraphProcess {
            inputs: input_block,
            outputs: output_block,
            frames,
            transport: Transport::default(),
            midi: &[],
        });
    }

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/audio/dsp_graph_latency_alignment.txt");
    let golden = read_golden(golden_path);
    assert_eq!(golden.len(), output.len());

    for (idx, (&expected, &actual)) in golden.iter().zip(output.iter()).enumerate() {
        let diff = (expected - actual).abs();
        assert!(
            diff < 1e-4,
            "sample {} differs: expected {}, got {}",
            idx,
            expected,
            actual
        );
    }
}
