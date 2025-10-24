use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use harmoniq_engine::{
    BufferConfig, ChannelLayout, GraphBuilder, HarmoniqEngine, NodeOsc, TransportState,
};

fn scene_48_tracks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(20);

    group.bench_function("48_tracks_96k_block64", |b| {
        let config = BufferConfig::new(96_000.0, 64, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");
        let mut graph = GraphBuilder::new();

        for track in 0..48 {
            let osc = NodeOsc::new(110.0 + track as f32).with_amplitude(0.02);
            let plugin = engine
                .register_processor(Box::new(osc))
                .expect("register processor");
            let node = graph.add_node(plugin);
            graph.connect_to_mixer(node, 1.0).expect("connect");
        }

        engine.replace_graph(graph.build()).expect("graph install");
        engine.reset_render_state().expect("reset");
        engine.set_transport(TransportState::Playing);
        let mut buffer = harmoniq_engine::AudioBuffer::from_config(&config);

        b.iter(|| {
            engine.process_block(&mut buffer).expect("process block");
        });
    });

    group.finish();
}

criterion_group!(benches, scene_48_tracks);
criterion_main!(benches);
