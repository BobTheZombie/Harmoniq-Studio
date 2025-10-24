use std::path::PathBuf;

use harmoniq_engine::render::{RenderDuration, RenderProject, RenderRequest, RenderSpeed};
use harmoniq_engine::{
    nodes::NodeOsc, AudioClip, BufferConfig, ChannelLayout, GraphBuilder, HarmoniqEngine,
};
use hound::{SampleFormat, WavReader};

const GOLDEN_FRAMES: usize = 4_800;
const SAMPLE_RATE: f32 = 48_000.0;
const CHANNELS: usize = 2;

struct GoldenProject;

impl RenderProject for GoldenProject {
    fn label(&self) -> &str {
        "golden-offline-sine"
    }

    fn create_engine(&self) -> anyhow::Result<HarmoniqEngine> {
        let config = BufferConfig::new(SAMPLE_RATE, 128, ChannelLayout::Stereo);
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

fn render_golden_clip() -> AudioClip {
    let project = GoldenProject;
    let mut renderer =
        harmoniq_engine::OfflineRenderer::new(project.create_engine().expect("engine"))
            .expect("renderer");

    let request = RenderRequest {
        duration: RenderDuration::Frames(GOLDEN_FRAMES),
        mixdown: None,
        stems: None,
        freeze: None,
        speed: RenderSpeed::Offline,
    };

    let result = renderer.render(&request).expect("render result");
    assert_eq!(result.mixdown.frames(), GOLDEN_FRAMES);
    result.mixdown
}

fn read_fixture(name: &str) -> (u32, Vec<Vec<f32>>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(name);
    let mut reader = WavReader::open(&path)
        .unwrap_or_else(|err| panic!("failed to open fixture {}: {}", path.display(), err));
    let spec = reader.spec();
    assert_eq!(spec.sample_format, SampleFormat::Float);
    let channel_count = spec.channels as usize;
    let mut channels = vec![Vec::new(); channel_count];
    for (index, sample) in reader.samples::<f32>().enumerate() {
        let value = sample.expect("sample");
        let channel = index % channel_count;
        channels[channel].push(value);
    }
    (spec.sample_rate, channels)
}

#[test]
fn offline_render_matches_golden_fixture() {
    let clip = render_golden_clip();
    assert_eq!(clip.channels(), CHANNELS);
    assert_eq!(clip.frames(), GOLDEN_FRAMES);

    let (sample_rate, golden_channels) = read_fixture("sine_stereo.wav");
    assert_eq!(sample_rate, SAMPLE_RATE as u32);
    assert_eq!(golden_channels.len(), CHANNELS);

    for channel in 0..CHANNELS {
        let rendered = clip.channel(channel).expect("channel");
        let golden = &golden_channels[channel];
        assert_eq!(golden.len(), rendered.len());
        for (frame, (&expected, &actual)) in golden.iter().zip(rendered.iter()).enumerate() {
            assert_eq!(
                expected.to_bits(),
                actual.to_bits(),
                "channel {} frame {} mismatch",
                channel,
                frame
            );
        }
    }
}

#[test]
fn offline_render_null_test() {
    let clip = render_golden_clip();
    let (_, golden_channels) = read_fixture("sine_stereo.wav");
    let (_, silence_channels) = read_fixture("silence_stereo.wav");

    for channel in 0..CHANNELS {
        let rendered = clip.channel(channel).expect("channel");
        let golden = &golden_channels[channel];
        let silence = &silence_channels[channel];
        assert_eq!(golden.len(), rendered.len());
        assert_eq!(silence.len(), rendered.len());

        for (frame, ((&expected, &actual), &silence_sample)) in golden
            .iter()
            .zip(rendered.iter())
            .zip(silence.iter())
            .enumerate()
        {
            assert_eq!(
                silence_sample.to_bits(),
                0,
                "silence fixture must be zero at frame {}",
                frame
            );
            let diff = actual - expected;
            let diff_bits = diff.to_bits();
            assert!(
                diff_bits == 0 || diff_bits == (-0.0f32).to_bits(),
                "channel {} frame {} did not null: diff {}",
                channel,
                frame,
                diff
            );
        }
    }
}
