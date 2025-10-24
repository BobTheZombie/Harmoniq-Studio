use harmoniq_engine::render::{RenderDuration, RenderProject, RenderRequest, RenderSpeed};
use harmoniq_engine::{
    nodes::NodeOsc, AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder,
    HarmoniqEngine, TransportState,
};

struct TestProject;

impl RenderProject for TestProject {
    fn label(&self) -> &str {
        "test-project"
    }

    fn create_engine(&self) -> anyhow::Result<HarmoniqEngine> {
        let config = BufferConfig::new(48_000.0, 128, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone())?;
        let mut builder = GraphBuilder::new();
        let osc = engine.register_processor(Box::new(NodeOsc::new(220.0).with_amplitude(0.25)))?;
        let node = builder.add_node(osc);
        builder.connect_to_mixer(node, 1.0)?;
        engine.replace_graph(builder.build())?;
        engine.reset_render_state()?;
        Ok(engine)
    }
}

#[test]
fn offline_render_matches_realtime_engine() {
    let project = TestProject;
    let offline_engine = project.create_engine().expect("engine");
    let mut renderer = harmoniq_engine::OfflineRenderer::new(offline_engine).expect("renderer");

    let request = RenderRequest {
        duration: RenderDuration::Frames(48_000),
        mixdown: None,
        stems: None,
        freeze: None,
        speed: RenderSpeed::Offline,
    };

    let result = renderer.render(&request).expect("render");
    let offline_clip = result.mixdown.clone();

    let mut realtime_engine = project.create_engine().expect("engine");
    realtime_engine
        .execute_command(EngineCommand::SetTransport(TransportState::Playing))
        .expect("transport");
    let mut buffer = AudioBuffer::from_config(realtime_engine.config());
    let mut realtime_channels = vec![Vec::new(); buffer.channel_count()];
    let mut remaining = 48_000;

    while remaining > 0 {
        realtime_engine
            .process_block(&mut buffer)
            .expect("realtime process");
        let frames = remaining.min(buffer.len());
        for channel in 0..buffer.channel_count() {
            realtime_channels[channel].extend_from_slice(&buffer.channel(channel)[..frames]);
        }
        remaining -= frames;
    }

    for channel in 0..offline_clip.channels() {
        let offline = offline_clip.channel(channel).expect("channel");
        let realtime = &realtime_channels[channel];
        assert_eq!(offline.len(), realtime.len());
        for (lhs, rhs) in offline.iter().zip(realtime.iter()) {
            let diff = (lhs - rhs).abs();
            assert!(diff < 1e-5, "channel {channel} differs ({lhs} vs {rhs})");
        }
    }
}
