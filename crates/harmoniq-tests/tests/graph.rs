use harmoniq_graph::{Graph, NodeKind, NodeParams, PinId};

#[test]
fn topology_and_pdc() {
    let mut graph = Graph::new();
    let input = graph.add_node(NodeKind::AudioInput, NodeParams::default());
    let track = graph.add_node(NodeKind::AudioTrack, NodeParams::default());
    let master = graph.add_node(NodeKind::MasterBus, NodeParams::default());

    graph.set_latency(track, 128);
    graph
        .connect(
            PinId {
                node: input,
                pin: 0,
            },
            PinId {
                node: track,
                pin: 0,
            },
            1.0,
        )
        .unwrap();
    graph
        .connect(
            PinId {
                node: track,
                pin: 0,
            },
            PinId {
                node: master,
                pin: 0,
            },
            1.0,
        )
        .unwrap();

    graph.recompute_pdc();
    let order = graph.topological_order();
    assert_eq!(order.len(), 3);
    assert_eq!(graph.pdc.max_latency, 128);
}
