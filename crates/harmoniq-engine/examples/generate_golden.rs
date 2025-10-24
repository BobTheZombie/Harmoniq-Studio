use std::fs::create_dir_all;
use std::path::PathBuf;

use harmoniq_engine::render::{RenderDuration, RenderProject, RenderRequest, RenderSpeed};
use harmoniq_engine::{
    nodes::NodeOsc, AudioClip, BufferConfig, ChannelLayout, GraphBuilder, HarmoniqEngine,
};
use hound::{SampleFormat, WavSpec, WavWriter};

const GOLDEN_FRAMES: usize = 4_800;
const SAMPLE_RATE: f32 = 48_000.0;
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

fn render_clip() -> AudioClip {
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

    renderer.render(&request).expect("render result").mixdown
}

fn write_clip(path: PathBuf, clip: &AudioClip) -> anyhow::Result<()> {
    let spec = WavSpec {
        channels: clip.channels() as u16,
        sample_rate: clip.sample_rate() as u32,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for frame in 0..clip.frames() {
        for channel in 0..clip.channels() {
            let sample = clip.channel(channel).expect("channel")[frame];
            writer.write_sample(sample)?;
        }
    }
    writer.finalize()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden");
    create_dir_all(&output_dir)?;

    let clip = render_clip();
    write_clip(output_dir.join("sine_stereo.wav"), &clip)?;

    let silence = AudioClip::with_sample_rate(
        clip.sample_rate(),
        vec![vec![0.0f32; clip.frames()]; clip.channels()],
    );
    write_clip(output_dir.join("silence_stereo.wav"), &silence)?;

    Ok(())
}
