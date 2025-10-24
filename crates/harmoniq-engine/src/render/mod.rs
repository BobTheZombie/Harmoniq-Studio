use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    engine::{HarmoniqEngine, TransportState},
    plugin::{PluginDescriptor, PluginId},
    AudioBuffer, AudioClip, BufferConfig, EngineCommand,
};

/// Audio file formats supported by the offline renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    Wav,
    Flac,
}

impl RenderFormat {
    fn extension(self) -> &'static str {
        match self {
            RenderFormat::Wav => "wav",
            RenderFormat::Flac => "flac",
        }
    }
}

/// Rendering speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSpeed {
    Offline,
    Realtime,
}

impl Default for RenderSpeed {
    fn default() -> Self {
        RenderSpeed::Offline
    }
}

/// Duration of a render request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderDuration {
    Frames(usize),
    Seconds(f32),
}

impl RenderDuration {
    fn frames(self, sample_rate: f32) -> usize {
        match self {
            RenderDuration::Frames(frames) => frames,
            RenderDuration::Seconds(seconds) => {
                (seconds.max(0.0) * sample_rate.max(f32::EPSILON)).round() as usize
            }
        }
    }
}

/// Dither algorithm applied before quantisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DitherKind {
    Tpdf,
}

/// Target audio file output.
#[derive(Debug, Clone)]
pub struct RenderFile {
    pub path: PathBuf,
    pub format: RenderFormat,
    pub dither: Option<DitherKind>,
}

impl RenderFile {
    fn ensure_parent(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create render directory {}", parent.display())
                })?;
            }
        }
        Ok(())
    }
}

/// Stem export configuration.
#[derive(Debug, Clone)]
pub struct StemSettings {
    pub directory: PathBuf,
    pub format: RenderFormat,
    pub dither: Option<DitherKind>,
    pub plugins: Option<Vec<PluginId>>,
}

impl StemSettings {
    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.directory).with_context(|| {
            format!(
                "failed to create stem directory {}",
                self.directory.display()
            )
        })
    }
}

/// Freeze request configuration.
#[derive(Debug, Clone)]
pub struct FreezeSettings {
    pub directory: PathBuf,
    pub format: RenderFormat,
    pub dither: Option<DitherKind>,
    pub plugins: Option<Vec<PluginId>>,
}

impl FreezeSettings {
    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.directory).with_context(|| {
            format!(
                "failed to create freeze directory {}",
                self.directory.display()
            )
        })
    }
}

/// Offline render request descriptor.
#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub duration: RenderDuration,
    pub mixdown: Option<RenderFile>,
    pub stems: Option<StemSettings>,
    pub freeze: Option<FreezeSettings>,
    pub speed: RenderSpeed,
}

impl Default for RenderRequest {
    fn default() -> Self {
        Self {
            duration: RenderDuration::Frames(0),
            mixdown: None,
            stems: None,
            freeze: None,
            speed: RenderSpeed::Offline,
        }
    }
}

/// Summary information for a completed render job.
#[derive(Debug, Clone)]
pub struct RenderReport {
    pub project: String,
    pub mixdown: Option<PathBuf>,
    pub stems: Vec<PathBuf>,
    pub freezes: Vec<PathBuf>,
    pub duration_frames: usize,
}

/// Offline render result containing audio clips before export.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub duration_frames: usize,
    pub mixdown: AudioClip,
    pub stems: Vec<StemRender>,
}

/// Captured stem render information.
#[derive(Debug, Clone)]
pub struct StemRender {
    pub plugin_id: PluginId,
    pub descriptor: PluginDescriptor,
    pub clip: AudioClip,
}

/// Trait implemented by structures capable of producing configured engines for rendering.
pub trait RenderProject: Send + Sync {
    fn label(&self) -> &str;
    fn create_engine(&self) -> Result<HarmoniqEngine>;
}

struct RenderJob {
    project: Arc<dyn RenderProject>,
    request: RenderRequest,
}

/// FIFO queue of render jobs.
#[derive(Default)]
pub struct RenderQueue {
    jobs: Vec<RenderJob>,
}

impl RenderQueue {
    pub fn new() -> Self {
        Self { jobs: Vec::new() }
    }

    pub fn enqueue_project<P>(&mut self, project: Arc<P>, request: RenderRequest)
    where
        P: RenderProject + 'static,
    {
        self.jobs.push(RenderJob { project, request });
    }

    pub fn process_all(mut self) -> Result<Vec<RenderReport>> {
        let mut reports = Vec::new();
        for job in self.jobs.drain(..) {
            let label = job.project.label().to_owned();
            let engine = job.project.create_engine()?;
            let mut renderer = OfflineRenderer::new(engine)?;
            let result = renderer.render(&job.request)?;
            let report = write_outputs(&label, &result, &job.request)?;
            reports.push(report);
        }
        Ok(reports)
    }
}

/// Offline renderer driving the Harmoniq engine faster than real-time.
pub struct OfflineRenderer {
    engine: HarmoniqEngine,
    config: BufferConfig,
}

impl OfflineRenderer {
    pub fn new(mut engine: HarmoniqEngine) -> Result<Self> {
        engine.reset_render_state()?;
        let config = engine.config().clone();
        Ok(Self { engine, config })
    }

    pub fn render(&mut self, request: &RenderRequest) -> Result<RenderResult> {
        let frames_to_render = request.duration.frames(self.config.sample_rate);
        if frames_to_render == 0 {
            return Ok(RenderResult {
                duration_frames: 0,
                mixdown: AudioClip::empty(
                    self.config.sample_rate,
                    self.config.layout.channels() as usize,
                ),
                stems: Vec::new(),
            });
        }

        let graph = self
            .engine
            .graph()
            .ok_or_else(|| anyhow!("project has no active processing graph"))?;
        let plugin_ids = graph.plugin_ids();

        let mut stem_buffers: Vec<Vec<Vec<f32>>> = Vec::with_capacity(plugin_ids.len());
        let mut descriptors = Vec::with_capacity(plugin_ids.len());
        for plugin_id in &plugin_ids {
            let descriptor = self
                .engine
                .plugin_descriptor(*plugin_id)
                .unwrap_or_else(|| PluginDescriptor::new("unknown", "Unknown", "Harmoniq"));
            stem_buffers.push(Vec::new());
            descriptors.push(descriptor);
        }

        let mut mixdown_channels = vec![Vec::new(); self.config.layout.channels() as usize];
        let mut remaining = frames_to_render;

        self.engine
            .execute_command(EngineCommand::SetTransport(TransportState::Playing))?;

        while remaining > 0 {
            let frames_this = remaining.min(self.config.block_size);
            let sleep = if matches!(request.speed, RenderSpeed::Realtime) {
                Some(Duration::from_secs_f32(
                    self.config.block_size as f32 / self.config.sample_rate,
                ))
            } else {
                None
            };

            self.engine.render_block_with(|master, scratch| {
                append_buffer(master, &mut mixdown_channels, frames_this);
                for (index, buffer) in scratch.iter().enumerate() {
                    if index >= stem_buffers.len() {
                        continue;
                    }
                    if stem_buffers[index].is_empty() {
                        stem_buffers[index] = vec![Vec::new(); buffer.channel_count()];
                    }
                    append_buffer(buffer, &mut stem_buffers[index], frames_this);
                }
            })?;

            remaining = remaining.saturating_sub(frames_this);

            if let Some(duration) = sleep {
                std::thread::sleep(duration);
            }
        }

        self.engine
            .execute_command(EngineCommand::SetTransport(TransportState::Stopped))?;

        let mixdown = AudioClip::with_sample_rate(self.config.sample_rate, mixdown_channels);
        let mut stems = Vec::with_capacity(plugin_ids.len());
        for ((plugin_id, descriptor), channels) in plugin_ids
            .into_iter()
            .zip(descriptors.into_iter())
            .zip(stem_buffers.into_iter())
        {
            if channels.is_empty() {
                continue;
            }
            stems.push(StemRender {
                plugin_id,
                descriptor,
                clip: AudioClip::with_sample_rate(self.config.sample_rate, channels),
            });
        }

        Ok(RenderResult {
            duration_frames: mixdown.frames(),
            mixdown,
            stems,
        })
    }
}

fn append_buffer(source: &AudioBuffer, destination: &mut Vec<Vec<f32>>, frames: usize) {
    let limit = frames.min(source.len());
    if limit == 0 {
        return;
    }
    let channels = source.channel_count();
    if destination.len() < channels {
        destination.resize_with(channels, Vec::new);
    }
    for channel_index in 0..channels {
        let channel = source.channel(channel_index);
        destination[channel_index].extend_from_slice(&channel[..limit]);
    }
}

struct TpdfDither {
    rng: StdRng,
    scale: f32,
}

impl TpdfDither {
    fn new(seed: u64, scale: f32) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            scale,
        }
    }

    fn sample(&mut self) -> f32 {
        let a: f32 = self.rng.gen();
        let b: f32 = self.rng.gen();
        (a - b) / self.scale
    }
}

fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_whitespace() || ch == '-' || ch == '_' {
            if !slug.ends_with('_') {
                slug.push('_');
            }
        }
    }
    slug.trim_matches('_').to_owned()
}

fn write_outputs(
    project: &str,
    result: &RenderResult,
    request: &RenderRequest,
) -> Result<RenderReport> {
    let mut mixdown_path = None;
    let mut stem_paths = Vec::new();
    let mut freeze_paths = Vec::new();

    if let Some(target) = &request.mixdown {
        target.ensure_parent()?;
        let path = target.path.clone();
        write_clip(&result.mixdown, target, 0)?;
        mixdown_path = Some(path);
    }

    if let Some(settings) = &request.stems {
        settings.ensure_dir()?;
        let allowed: Option<HashSet<PluginId>> = settings
            .plugins
            .as_ref()
            .map(|ids| ids.iter().cloned().collect());
        for stem in &result.stems {
            if let Some(allowed) = &allowed {
                if !allowed.contains(&stem.plugin_id) {
                    continue;
                }
            }
            let mut file_name = slugify(&stem.descriptor.name);
            if file_name.is_empty() {
                file_name = format!("stem_{}", stem.plugin_id.0);
            }
            let path =
                settings
                    .directory
                    .join(format!("{}.{}", file_name, settings.format.extension()));
            let target = RenderFile {
                path: path.clone(),
                format: settings.format,
                dither: settings.dither,
            };
            write_clip(&stem.clip, &target, stem.plugin_id.0)?;
            stem_paths.push(path);
        }
    }

    if let Some(settings) = &request.freeze {
        settings.ensure_dir()?;
        let allowed: Option<HashSet<PluginId>> = settings
            .plugins
            .as_ref()
            .map(|ids| ids.iter().cloned().collect());
        for stem in &result.stems {
            if let Some(allowed) = &allowed {
                if !allowed.contains(&stem.plugin_id) {
                    continue;
                }
            }
            let mut file_name = slugify(&stem.descriptor.name);
            if file_name.is_empty() {
                file_name = format!("freeze_{}", stem.plugin_id.0);
            } else {
                file_name = format!("{}_freeze", file_name);
            }
            let path =
                settings
                    .directory
                    .join(format!("{}.{}", file_name, settings.format.extension()));
            let target = RenderFile {
                path: path.clone(),
                format: settings.format,
                dither: settings.dither,
            };
            write_clip(&stem.clip, &target, stem.plugin_id.0 ^ 0xDEADBEEF)?;
            freeze_paths.push(path);
        }
    }

    Ok(RenderReport {
        project: project.to_owned(),
        mixdown: mixdown_path,
        stems: stem_paths,
        freezes: freeze_paths,
        duration_frames: result.duration_frames,
    })
}

fn write_clip(clip: &AudioClip, target: &RenderFile, seed: u64) -> Result<()> {
    target.ensure_parent()?;
    match target.format {
        RenderFormat::Wav => write_wav(clip, target, seed),
        RenderFormat::Flac => write_flac(clip, target, seed),
    }
}

fn write_wav(clip: &AudioClip, target: &RenderFile, seed: u64) -> Result<()> {
    use hound::{SampleFormat, WavSpec, WavWriter};

    let spec = WavSpec {
        channels: clip.channels() as u16,
        sample_rate: clip.sample_rate() as u32,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };

    let writer = WavWriter::create(&target.path, spec)
        .with_context(|| format!("failed to create {}", target.path.display()))?;
    let mut writer = writer;

    let mut dither = target.dither.map(|kind| match kind {
        DitherKind::Tpdf => TpdfDither::new(seed, I24_MAX as f32),
    });

    let frames = clip.frames();
    for frame in 0..frames {
        for channel in 0..clip.channels() {
            let sample = clip
                .channel(channel)
                .and_then(|channel| channel.get(frame))
                .copied()
                .unwrap_or(0.0);
            let quantised = quantise_sample(sample, dither.as_mut());
            writer.write_sample(quantised)?;
        }
    }

    writer.finalize()?;
    Ok(())
}

fn write_flac(clip: &AudioClip, target: &RenderFile, seed: u64) -> Result<()> {
    use flacenc::bitsink::ByteSink;
    use flacenc::component::BitRepr;
    use flacenc::config::Encoder as FlacEncoder;
    use flacenc::error::Verify;
    use flacenc::source::MemSource;

    let channels = clip.channels();
    let sample_rate = clip.sample_rate() as usize;
    let frames = clip.frames();

    let mut dither = target.dither.map(|kind| match kind {
        DitherKind::Tpdf => TpdfDither::new(seed, I24_MAX as f32),
    });

    let mut buffer: Vec<i32> = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        for channel in 0..channels {
            let sample = clip
                .channel(channel)
                .and_then(|channel| channel.get(frame))
                .copied()
                .unwrap_or(0.0);
            buffer.push(quantise_sample(sample, dither.as_mut()));
        }
    }

    let config = FlacEncoder::default()
        .into_verified()
        .map_err(|(_, err)| anyhow!("invalid FLAC encoder configuration: {err}"))?;
    let source = MemSource::from_samples(&buffer, channels, 24, sample_rate);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|err| anyhow!("failed to encode FLAC stream: {err:?}"))?;

    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|err| anyhow!("failed to serialise FLAC stream: {err:?}"))?;
    fs::write(&target.path, sink.as_slice())
        .with_context(|| format!("failed to write {}", target.path.display()))?;
    Ok(())
}

const I24_MAX: i32 = 0x7F_FFFF;

fn quantise_sample(sample: f32, dither: Option<&mut TpdfDither>) -> i32 {
    let mut value = sample;
    if let Some(dither) = dither {
        value += dither.sample();
    }
    let scaled = (value * I24_MAX as f32).round();
    scaled.clamp(-(I24_MAX as f32), I24_MAX as f32).trunc() as i32
}
