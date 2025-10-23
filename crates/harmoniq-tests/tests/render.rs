use harmoniq_engine::render_offline;
use harmoniq_graph::{Graph, NodeKind, NodeParams, PinId};

#[test]
fn render_chain_alignment() {
    let mut graph = Graph::new();
    let input = graph.add_node(NodeKind::AudioInput, NodeParams::default());
    let plug1 = graph.add_node(
        NodeKind::PluginContainer(Default::default()),
        NodeParams::default(),
    );
    let plug2 = graph.add_node(
        NodeKind::PluginContainer(Default::default()),
        NodeParams::default(),
    );
    let plug3 = graph.add_node(
        NodeKind::PluginContainer(Default::default()),
        NodeParams::default(),
    );
    let output = graph.add_node(NodeKind::AudioOutput, NodeParams::default());

    graph.set_latency(plug1, 64);
    graph.set_latency(plug2, 128);
    graph.set_latency(plug3, 32);

    graph
        .connect(
            PinId {
                node: input,
                pin: 0,
            },
            PinId {
                node: plug1,
                pin: 0,
            },
            1.0,
        )
        .unwrap();
    graph
        .connect(
            PinId {
                node: plug1,
                pin: 0,
            },
            PinId {
                node: plug2,
                pin: 0,
            },
            1.0,
        )
        .unwrap();
    graph
        .connect(
            PinId {
                node: plug2,
                pin: 0,
            },
            PinId {
                node: plug3,
                pin: 0,
            },
            1.0,
        )
        .unwrap();
    graph
        .connect(
            PinId {
                node: plug3,
                pin: 0,
            },
            PinId {
                node: output,
                pin: 0,
            },
            1.0,
        )
        .unwrap();

    graph.recompute_pdc();
    let blocks = render_offline(graph.clone(), 3);
    assert_eq!(blocks.len(), 3);
    for block in blocks {
        assert_eq!(block.frames, 256);
    }
    assert!(graph.pdc.max_latency >= 128);
}
