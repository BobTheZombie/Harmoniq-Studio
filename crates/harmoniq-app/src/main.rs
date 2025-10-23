use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use flacenc::component::BitRepr;
use flacenc::error::Verify;

use anyhow::{anyhow, Context};
use clap::Parser;
use eframe::egui::{
    self, Align2, CursorIcon, Id, Margin, PointerButton, RichText, Rounding, Stroke,
};
use eframe::{App, CreationContext, NativeOptions};
use egui_dock::{DockArea, DockState, Style as DockStyle, TabViewer};
use egui_extras::{image::load_svg_bytes, install_image_loaders};
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine,
    TransportState,
};
use harmoniq_plugins::{GainPlugin, NoisePlugin, SineSynth};
use harmoniq_ui::{HarmoniqPalette, HarmoniqTheme};
use hound::{SampleFormat, WavSpec, WavWriter};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;

mod audio;
mod midi;
mod ui;

use audio::{
    available_backends, describe_layout, AudioBackend, AudioRuntimeOptions, RealtimeAudio,
};
use midi::list_midi_inputs;
use ui::{
    audio_settings::{ActiveAudioSummary, AudioSettingsAction, AudioSettingsPanel},
    browser::BrowserPane,
    channel_rack::ChannelRackPane,
    event_bus::{AppEvent, EventBus, LayoutEvent, TransportEvent},
    layout::LayoutState,
    menu::MenuBarState,
    mixer::MixerPane,
    piano_roll::PianoRollPane,
    playlist::PlaylistPane,
    transport::{TransportBar, TransportSnapshot},
    workspace::{build_default_workspace, WorkspacePane},
};

#[derive(Debug, Parser)]
#[command(author, version, about = "Harmoniq Studio prototype")]
struct Cli {
    /// Run without the native UI, performing a short offline render
    #[arg(long)]
    headless: bool,

    /// Export an offline bounce to the provided audio file path (.wav/.flac/.mp3)
    #[arg(long)]
    bounce: Option<PathBuf>,

    /// Number of beats rendered when performing an offline bounce
    #[arg(long, default_value_t = 16.0)]
    bounce_beats: f32,

    /// Sample rate used for the audio engine
    #[arg(long, default_value_t = 48_000.0)]
    sample_rate: f32,

    /// Block size used for internal processing
    #[arg(long, default_value_t = 512)]
    block_size: usize,

    /// Initial tempo used for transport and offline bouncing
    #[arg(long, default_value_t = 120.0)]
    tempo: f32,

    /// Preferred realtime audio backend
    #[arg(long, default_value_t = AudioBackend::Auto)]
    audio_backend: AudioBackend,

    /// Disable realtime audio streaming
    #[arg(long, default_value_t = false)]
    disable_audio: bool,

    /// Name of the MIDI controller or input port to connect
    #[arg(long)]
    midi_input: Option<String>,

    /// List available realtime audio backends
    #[arg(long, default_value_t = false)]
    list_audio_backends: bool,

    /// List available MIDI input ports
    #[arg(long, default_value_t = false)]
    list_midi_inputs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DemoGraphConfig {
    sine_frequency: f32,
    sine_mix: f32,
    noise_mix: f32,
    gain: f32,
}

impl DemoGraphConfig {
    fn headless_default() -> Self {
        Self {
            sine_frequency: 220.0,
            sine_mix: 0.7,
            noise_mix: 0.1,
            gain: 0.4,
        }
    }

    fn ui_default() -> Self {
        Self {
            sine_frequency: 110.0,
            sine_mix: 0.8,
            noise_mix: 0.2,
            gain: 0.6,
        }
    }
}

fn configure_demo_graph(
    engine: &mut HarmoniqEngine,
    graph_config: &DemoGraphConfig,
) -> anyhow::Result<()> {
    let sine = engine
        .register_processor(Box::new(SineSynth::with_frequency(
            graph_config.sine_frequency,
        )))
        .context("register sine")?;
    let noise = engine
        .register_processor(Box::new(NoisePlugin::default()))
        .context("register noise")?;
    let gain = engine
        .register_processor(Box::new(GainPlugin::new(graph_config.gain)))
        .context("register gain")?;

    let mut graph_builder = GraphBuilder::new();
    let sine_node = graph_builder.add_node(sine);
    graph_builder.connect_to_mixer(sine_node, graph_config.sine_mix)?;
    let noise_node = graph_builder.add_node(noise);
    graph_builder.connect_to_mixer(noise_node, graph_config.noise_mix)?;
    let gain_node = graph_builder.add_node(gain);
    graph_builder.connect_to_mixer(gain_node, 1.0)?;

    engine.replace_graph(graph_builder.build())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioExportFormat {
    Wav,
    Flac,
    Mp3,
}

impl AudioExportFormat {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "wav" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            "mp3" => Some(Self::Mp3),
            _ => None,
        }
    }

    fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Wav => "WAV",
            Self::Flac => "FLAC",
            Self::Mp3 => "MP3",
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppIcons {
    play: egui::TextureHandle,
    pause: egui::TextureHandle,
    stop: egui::TextureHandle,
    save: egui::TextureHandle,
    open: egui::TextureHandle,
    bounce: egui::TextureHandle,
    track: egui::TextureHandle,
    clip: egui::TextureHandle,
    automation: egui::TextureHandle,
    tempo: egui::TextureHandle,
    settings: egui::TextureHandle,
}

impl AppIcons {
    fn load(ctx: &egui::Context) -> anyhow::Result<Self> {
        fn load_svg(
            ctx: &egui::Context,
            name: &str,
            bytes: &[u8],
        ) -> anyhow::Result<egui::TextureHandle> {
            let image = load_svg_bytes(bytes)
                .map_err(|err| anyhow!("failed to load icon {name}: {err}"))?;
            Ok(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
        }

        Ok(Self {
            play: load_svg(
                ctx,
                "icon_play",
                include_bytes!("../../../resources/icons/play.svg"),
            )?,
            pause: load_svg(
                ctx,
                "icon_pause",
                include_bytes!("../../../resources/icons/pause.svg"),
            )?,
            stop: load_svg(
                ctx,
                "icon_stop",
                include_bytes!("../../../resources/icons/stop.svg"),
            )?,
            save: load_svg(
                ctx,
                "icon_save",
                include_bytes!("../../../resources/icons/save.svg"),
            )?,
            open: load_svg(
                ctx,
                "icon_open",
                include_bytes!("../../../resources/icons/folder.svg"),
            )?,
            bounce: load_svg(
                ctx,
                "icon_bounce",
                include_bytes!("../../../resources/icons/bounce.svg"),
            )?,
            track: load_svg(
                ctx,
                "icon_track",
                include_bytes!("../../../resources/icons/track.svg"),
            )?,
            clip: load_svg(
                ctx,
                "icon_clip",
                include_bytes!("../../../resources/icons/clip.svg"),
            )?,
            automation: load_svg(
                ctx,
                "icon_automation",
                include_bytes!("../../../resources/icons/automation.svg"),
            )?,
            tempo: load_svg(
                ctx,
                "icon_tempo",
                include_bytes!("../../../resources/icons/tempo.svg"),
            )?,
            settings: load_svg(
                ctx,
                "icon_settings",
                include_bytes!("../../../resources/icons/harmoniq-studio.svg"),
            )?,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    pub fn as_tuple(&self) -> (u32, u32) {
        (self.numerator, self.denominator)
    }

    pub fn set_from_tuple(&mut self, value: (u32, u32)) {
        self.numerator = value.0.max(1);
        self.denominator = value.1.max(1);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TransportClock {
    pub bars: u32,
    pub beats: u32,
    pub ticks: u32,
}

impl TransportClock {
    fn from_beats(position_beats: f32, signature: TimeSignature) -> Self {
        let ticks_per_beat = 960u32;
        let total_ticks = (position_beats.max(0.0) * ticks_per_beat as f32).round() as u32;
        let ticks_per_bar = signature.numerator.max(1) * ticks_per_beat;
        let bars = total_ticks / ticks_per_bar;
        let bar_remainder = total_ticks % ticks_per_bar;
        let beats = bar_remainder / ticks_per_beat;
        let ticks = bar_remainder % ticks_per_beat;
        Self {
            bars: bars + 1,
            beats: beats + 1,
            ticks,
        }
    }

    fn format(&self) -> String {
        format!("{:02}:{:02}:{:03}", self.bars, self.beats, self.ticks)
    }
}

#[derive(Debug, Clone)]
struct EngineContext {
    tempo: f32,
    time_signature: TimeSignature,
    transport: TransportState,
    cpu_usage: f32,
    clock: TransportClock,
    master_meter: (f32, f32),
}

impl EngineContext {
    fn new(tempo: f32, time_signature: TimeSignature) -> Self {
        Self {
            tempo,
            time_signature,
            transport: TransportState::Stopped,
            cpu_usage: 0.0,
            clock: TransportClock::default(),
            master_meter: (0.0, 0.0),
        }
    }
}

fn write_wav_file(
    path: &Path,
    channels: usize,
    sample_rate: f32,
    samples: &[i16],
) -> anyhow::Result<()> {
    let spec = WavSpec {
        channels: channels as u16,
        sample_rate: sample_rate.round() as u32,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).context("failed to create output WAV file")?;
    for sample in samples {
        writer
            .write_sample(*sample)
            .context("failed to write WAV sample")?;
    }
    writer.finalize().context("failed to finalise WAV writer")?;
    Ok(())
}

fn write_flac_file(
    path: &Path,
    channels: usize,
    sample_rate: f32,
    samples: &[i16],
) -> anyhow::Result<()> {
    let pcm: Vec<i32> = samples.iter().map(|value| i32::from(*value)).collect();
    let mut encoder_config = flacenc::config::Encoder::default();
    if encoder_config.block_size == 0 {
        encoder_config.block_size = 4096;
    }
    let encoder_config = encoder_config
        .into_verified()
        .map_err(|(_, err)| anyhow!("invalid FLAC encoder configuration: {err:?}"))?;
    let block_size = encoder_config.block_size;
    let source =
        flacenc::source::MemSource::from_samples(&pcm, channels, 16, sample_rate.round() as usize);
    let stream = flacenc::encode_with_fixed_block_size(&encoder_config, source, block_size)
        .map_err(|err| anyhow!("failed to encode FLAC stream: {err:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|err| anyhow!("failed to write FLAC stream: {err:?}"))?;
    fs::write(path, sink.as_slice()).context("failed to write FLAC file")?;
    Ok(())
}

fn write_mp3_file(
    path: &Path,
    channels: usize,
    sample_rate: f32,
    samples: &[i16],
) -> anyhow::Result<()> {
    let mut builder = mp3lame_encoder::Builder::new()
        .ok_or_else(|| anyhow!("failed to create MP3 encoder builder"))?;
    builder
        .set_sample_rate(sample_rate.round() as u32)
        .map_err(|err| anyhow!("invalid sample rate for MP3 encoder: {err}"))?;
    builder
        .set_num_channels(channels as u8)
        .map_err(|err| anyhow!("invalid channel count for MP3 encoder: {err}"))?;
    builder
        .set_quality(mp3lame_encoder::Quality::Good)
        .map_err(|err| anyhow!("failed to configure MP3 encoder quality: {err}"))?;

    let mut encoder = builder
        .build()
        .map_err(|err| anyhow!("failed to create MP3 encoder: {err}"))?;

    let pcm: Vec<i16> = samples.to_vec();
    let mut output = Vec::new();
    let frame_count = samples.len() / channels.max(1);
    output.reserve(mp3lame_encoder::max_required_buffer_size(frame_count));

    encoder
        .encode_to_vec(mp3lame_encoder::InterleavedPcm(&pcm), &mut output)
        .map_err(|err| anyhow!("failed to encode MP3: {err}"))?;
    encoder
        .flush_to_vec::<mp3lame_encoder::FlushNoGap>(&mut output)
        .map_err(|err| anyhow!("failed to finalise MP3: {err}"))?;

    fs::write(path, output).context("failed to write MP3 file")?;
    Ok(())
}

fn offline_bounce_to_file(
    output_path: impl AsRef<Path>,
    config: BufferConfig,
    graph_config: &DemoGraphConfig,
    tempo: f32,
    beats: f32,
) -> anyhow::Result<PathBuf> {
    if beats <= 0.0 {
        anyhow::bail!("bounce length in beats must be positive");
    }

    let mut path = output_path.as_ref().to_path_buf();
    let format = match AudioExportFormat::from_path(&path) {
        Some(format) => format,
        None => {
            if path.extension().is_none() {
                path.set_extension("wav");
                AudioExportFormat::Wav
            } else {
                let ext = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or_default();
                anyhow::bail!("unsupported export format: '{ext}'");
            }
        }
    };

    let total_seconds = (60.0 / tempo.max(1.0)) * beats;
    let total_frames = (total_seconds * config.sample_rate) as usize;
    let channels = config.layout.channels() as usize;

    let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
    configure_demo_graph(&mut engine, graph_config)?;
    engine.set_transport(TransportState::Playing);

    let mut buffer = AudioBuffer::from_config(config.clone());
    let mut frames_written = 0usize;
    let mut samples: Vec<i16> = Vec::with_capacity(total_frames * channels);

    while frames_written < total_frames {
        engine.process_block(&mut buffer)?;
        let block_len = buffer.len();
        let channel_data = buffer.as_slice();

        for frame in 0..block_len {
            if frames_written >= total_frames {
                break;
            }
            for channel in 0..channels {
                let sample = channel_data[channel][frame].clamp(-1.0, 1.0);
                let quantized = (sample * i16::MAX as f32) as i16;
                samples.push(quantized);
            }
            frames_written += 1;
        }
    }

    match format {
        AudioExportFormat::Wav => write_wav_file(&path, channels, config.sample_rate, &samples)?,
        AudioExportFormat::Flac => write_flac_file(&path, channels, config.sample_rate, &samples)?,
        AudioExportFormat::Mp3 => write_mp3_file(&path, channels, config.sample_rate, &samples)?,
    }

    Ok(path)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Cli::parse();

    if args.list_audio_backends {
        let backends = available_backends();
        println!("Available realtime audio backends:");
        for (backend, description) in backends {
            println!("  - {backend}: {description}");
        }
        return Ok(());
    }

    if args.list_midi_inputs {
        let ports = list_midi_inputs()?;
        if ports.is_empty() {
            println!("No MIDI input ports detected.");
        } else {
            println!("Available MIDI inputs:");
            for port in ports {
                println!("  - {port}");
            }
        }
        return Ok(());
    }

    let runtime_options = AudioRuntimeOptions::new(
        args.audio_backend,
        args.midi_input.clone(),
        !args.disable_audio,
    );

    if let Some(path) = args.bounce.clone() {
        let config = BufferConfig::new(args.sample_rate, args.block_size, ChannelLayout::Stereo);
        let bounced_path = offline_bounce_to_file(
            path,
            config,
            &DemoGraphConfig::headless_default(),
            args.tempo,
            args.bounce_beats,
        )?;
        println!("Offline bounce written to {}", bounced_path.display());
        return Ok(());
    }

    if args.headless {
        run_headless(&args, runtime_options)
    } else {
        run_ui(&args, runtime_options)
    }
}

fn run_headless(args: &Cli, runtime: AudioRuntimeOptions) -> anyhow::Result<()> {
    let config = BufferConfig::new(args.sample_rate, args.block_size, ChannelLayout::Stereo);
    let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
    configure_demo_graph(&mut engine, &DemoGraphConfig::headless_default())?;
    engine.set_transport(TransportState::Playing);
    engine.execute_command(EngineCommand::SetTempo(args.tempo))?;

    let command_queue = engine.command_queue();
    let engine = Arc::new(Mutex::new(engine));

    if runtime.is_enabled() {
        match RealtimeAudio::start(
            Arc::clone(&engine),
            command_queue,
            config.clone(),
            runtime.clone(),
        ) {
            Ok(stream) => {
                let backend_label = stream
                    .host_label()
                    .map(|label| label.to_string())
                    .unwrap_or_else(|| stream.backend().to_string());
                println!(
                    "Streaming realtime audio via {} on '{}' ({} layout) – press Ctrl+C to stop.",
                    backend_label,
                    stream.device_name(),
                    describe_layout(config.layout)
                );

                let running = Arc::new(AtomicBool::new(true));
                let running_clone = Arc::clone(&running);
                ctrlc::set_handler(move || {
                    running_clone.store(false, AtomicOrdering::SeqCst);
                })?;

                while running.load(AtomicOrdering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "failed to start realtime audio output; engine will run offline instead"
                );
                println!(
                    "Realtime audio unavailable: {:#}. Running an offline preview instead.",
                    err
                );
                run_headless_offline_preview(&engine, &config, args.tempo)?;
            }
        }
    } else {
        run_headless_offline_preview(&engine, &config, args.tempo)?;
    }

    Ok(())
}

fn run_headless_offline_preview(
    engine: &Arc<Mutex<HarmoniqEngine>>,
    config: &BufferConfig,
    tempo: f32,
) -> anyhow::Result<()> {
    let mut buffer = AudioBuffer::from_config(config.clone());
    for _ in 0..10 {
        {
            let mut engine = engine.lock();
            engine.process_block(&mut buffer)?;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    println!(
        "Rendered {} frames across {} channels at {:.1} BPM",
        buffer.len(),
        config.layout.channels(),
        tempo
    );

    Ok(())
}

fn run_ui(args: &Cli, runtime: AudioRuntimeOptions) -> anyhow::Result<()> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    let config = BufferConfig::new(args.sample_rate, args.block_size, ChannelLayout::Stereo);
    let config_for_app = config.clone();
    let tempo_for_app = args.tempo;
    let runtime_for_app = runtime.clone();

    eframe::run_native(
        "Harmoniq Studio",
        native_options,
        Box::new(move |cc| {
            install_image_loaders(&cc.egui_ctx);
            let config = config_for_app.clone();
            let app =
                match HarmoniqStudioApp::new(config, tempo_for_app, runtime_for_app.clone(), cc) {
                    Ok(app) => app,
                    Err(err) => {
                        eprintln!("Failed to initialise Harmoniq Studio UI: {err:?}");
                        process::exit(1);
                    }
                };
            Box::new(app)
        }),
    )
    .map_err(|err| anyhow!(err.to_string()))
}

struct EngineRunner {
    engine: Arc<Mutex<HarmoniqEngine>>,
    config: BufferConfig,
    command_queue: harmoniq_engine::EngineCommandQueue,
    runtime: AudioRuntimeOptions,
    realtime: Option<RealtimeAudio>,
    offline_loop: Option<OfflineLoop>,
    last_runtime_error: Option<String>,
}

impl EngineRunner {
    fn start(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        command_queue: harmoniq_engine::EngineCommandQueue,
        runtime: AudioRuntimeOptions,
    ) -> anyhow::Result<Self> {
        let mut runner = Self {
            engine,
            config,
            command_queue,
            runtime,
            realtime: None,
            offline_loop: None,
            last_runtime_error: None,
        };
        if let Err(err) = runner.refresh_runtime() {
            warn!(
                error = %err,
                "failed to start realtime audio; engine will run offline instead"
            );
        }
        Ok(runner)
    }

    fn stop_streams(&mut self) {
        if let Some(mut loop_state) = self.offline_loop.take() {
            loop_state.stop();
        }
        self.realtime = None;
        self.last_runtime_error = None;
    }

    fn refresh_runtime(&mut self) -> anyhow::Result<()> {
        self.stop_streams();

        if self.runtime.is_enabled() {
            let options = self.runtime.clone();
            match RealtimeAudio::start(
                Arc::clone(&self.engine),
                self.command_queue.clone(),
                self.config.clone(),
                options,
            ) {
                Ok(realtime) => {
                    if let Some(id) = realtime.device_id() {
                        self.runtime.output_device = Some(id.to_string());
                    }
                    self.realtime = Some(realtime);
                    Ok(())
                }
                Err(err) => {
                    self.last_runtime_error = Some(format!("{err:#}"));
                    self.runtime.enable_audio = false;
                    self.offline_loop = Some(OfflineLoop::start(
                        Arc::clone(&self.engine),
                        self.config.clone(),
                    ));
                    Err(err)
                }
            }
        } else {
            self.offline_loop = Some(OfflineLoop::start(
                Arc::clone(&self.engine),
                self.config.clone(),
            ));
            Ok(())
        }
    }

    fn try_reconfigure(
        &mut self,
        config: BufferConfig,
        runtime: AudioRuntimeOptions,
    ) -> anyhow::Result<()> {
        self.stop_streams();
        {
            let mut engine = self.engine.lock();
            engine.reconfigure(config.clone())?;
        }
        self.config = config;
        self.runtime = runtime;
        self.refresh_runtime()
    }

    fn reconfigure(
        &mut self,
        config: BufferConfig,
        runtime: AudioRuntimeOptions,
    ) -> anyhow::Result<()> {
        let previous_config = self.config.clone();
        let previous_runtime = self.runtime.clone();

        match self.try_reconfigure(config, runtime) {
            Ok(()) => Ok(()),
            Err(err) => {
                if let Err(revert_err) = self.try_reconfigure(previous_config, previous_runtime) {
                    error!(
                        ?revert_err,
                        "failed to restore previous audio configuration after error",
                    );
                }
                Err(err)
            }
        }
    }

    fn reconfigure_audio(&mut self, runtime: AudioRuntimeOptions) -> anyhow::Result<()> {
        let previous = self.runtime.clone();
        self.runtime = runtime;
        if let Err(err) = self.refresh_runtime() {
            self.runtime = previous;
            if let Err(revert) = self.refresh_runtime() {
                error!(
                    ?revert,
                    "failed to restore previous audio runtime after error",
                );
            }
            return Err(err);
        }
        Ok(())
    }

    fn config(&self) -> &BufferConfig {
        &self.config
    }

    fn runtime_options(&self) -> &AudioRuntimeOptions {
        &self.runtime
    }

    fn realtime(&self) -> Option<&RealtimeAudio> {
        self.realtime.as_ref()
    }

    fn last_runtime_error(&self) -> Option<&str> {
        self.last_runtime_error.as_deref()
    }
}

struct OfflineLoop {
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl OfflineLoop {
    fn start(engine: Arc<Mutex<HarmoniqEngine>>, config: BufferConfig) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let handle = std::thread::spawn(move || {
            let mut buffer = AudioBuffer::from_config(config);
            while running_clone.load(AtomicOrdering::SeqCst) {
                {
                    let mut engine = engine.lock();
                    if let Err(err) = engine.process_block(&mut buffer) {
                        error!("error while processing offline audio: {err:#}");
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        Self {
            running,
            handle: Some(handle),
        }
    }

    fn stop(&mut self) {
        self.running.store(false, AtomicOrdering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for OfflineLoop {
    fn drop(&mut self) {
        self.stop();
    }
}

struct HarmoniqStudioApp {
    theme: HarmoniqTheme,
    icons: AppIcons,
    dock_style: DockStyle,
    dock_state: DockState<WorkspacePane>,
    layout: LayoutState,
    menu: MenuBarState,
    transport_bar: TransportBar,
    browser: BrowserPane,
    channel_rack: ChannelRackPane,
    piano_roll: PianoRollPane,
    mixer: MixerPane,
    playlist: PlaylistPane,
    audio_settings: AudioSettingsPanel,
    event_bus: EventBus,
    engine_runner: EngineRunner,
    command_queue: harmoniq_engine::EngineCommandQueue,
    engine_context: Arc<Mutex<EngineContext>>,
    tempo: f32,
    time_signature: TimeSignature,
    transport_state: TransportState,
    transport_clock: TransportClock,
    metronome: bool,
    pattern_mode: bool,
    last_engine_update: Instant,
    status_message: Option<String>,
}

impl HarmoniqStudioApp {
    fn new(
        config: BufferConfig,
        initial_tempo: f32,
        runtime: AudioRuntimeOptions,
        cc: &CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        let theme = HarmoniqTheme::init(&cc.egui_ctx);
        let dock_style = Self::create_dock_style(cc.egui_ctx.style().as_ref(), theme.palette());
        let icons = AppIcons::load(&cc.egui_ctx)?;

        let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
        let graph_config = DemoGraphConfig::ui_default();
        configure_demo_graph(&mut engine, &graph_config)?;
        engine.set_transport(TransportState::Stopped);
        engine.execute_command(EngineCommand::SetTempo(initial_tempo))?;

        let command_queue = engine.command_queue();
        let engine = Arc::new(Mutex::new(engine));
        let engine_runner = EngineRunner::start(
            Arc::clone(&engine),
            config.clone(),
            command_queue.clone(),
            runtime,
        )?;

        let time_signature = TimeSignature::default();
        let engine_context = Arc::new(Mutex::new(EngineContext::new(
            initial_tempo,
            time_signature,
        )));

        let layout = LayoutState::load(PathBuf::from("config/ui_layout.json"));
        let mut dock_state = layout.dock().unwrap_or_else(|| build_default_workspace());

        if dock_state.main_surface().is_empty() {
            dock_state = build_default_workspace();
        }

        let resources_root = env::current_dir()
            .map(|path| path.join("resources"))
            .unwrap_or_else(|_| PathBuf::from("resources"));

        let event_bus = EventBus::default();
        let menu = MenuBarState::default();
        let transport_bar = TransportBar::new(initial_tempo, time_signature);
        let browser = BrowserPane::new(
            resources_root,
            layout.browser_visible(),
            layout.browser_width(),
        );
        let channel_rack = ChannelRackPane::default();
        let piano_roll = PianoRollPane::default();
        let mixer = MixerPane::default();
        let playlist = PlaylistPane::default();
        let audio_settings =
            AudioSettingsPanel::new(engine_runner.config(), engine_runner.runtime_options());

        let status_message = engine_runner
            .last_runtime_error()
            .map(|err| format!("Realtime audio unavailable: {err}. Running offline."));

        Ok(Self {
            theme,
            icons,
            dock_style,
            dock_state,
            layout,
            menu,
            transport_bar,
            browser,
            channel_rack,
            piano_roll,
            mixer,
            playlist,
            audio_settings,
            event_bus,
            engine_runner,
            command_queue,
            engine_context,
            tempo: initial_tempo,
            time_signature,
            transport_state: TransportState::Stopped,
            transport_clock: TransportClock::default(),
            metronome: false,
            pattern_mode: true,
            last_engine_update: Instant::now(),
            status_message,
        })
    }

    fn create_dock_style(base_style: &egui::Style, palette: &HarmoniqPalette) -> DockStyle {
        let mut style = DockStyle::from_egui(base_style);

        style.main_surface_border_stroke = Stroke::new(1.0, palette.toolbar_outline);
        style.main_surface_border_rounding = Rounding::same(12.0);

        style.tab_bar.bg_fill = palette.panel_alt;
        style.tab_bar.hline_color = palette.toolbar_outline;
        style.tab_bar.fill_tab_bar = true;

        style.tab.tab_body.bg_fill = palette.panel;
        style.tab.tab_body.stroke = Stroke::new(1.0, palette.toolbar_outline);
        style.tab.tab_body.rounding = Rounding::same(12.0);

        style.tab.active.bg_fill = palette.panel_alt;
        style.tab.active.text_color = palette.text_primary;
        style.tab.active.outline_color = palette.toolbar_outline;

        style.tab.hovered.bg_fill = palette.toolbar_highlight;
        style.tab.hovered.text_color = palette.text_primary;
        style.tab.hovered.outline_color = palette.accent;

        style.tab.focused = style.tab.active.clone();
        style.tab.focused_with_kb_focus = style.tab.active.clone();
        style.tab.active_with_kb_focus = style.tab.active.clone();

        style.tab.inactive.bg_fill = palette.panel;
        style.tab.inactive.text_color = palette.text_muted;
        style.tab.inactive.outline_color = palette.toolbar_outline;
        style.tab.inactive_with_kb_focus = style.tab.inactive.clone();

        style.buttons.close_tab_color = palette.text_primary;
        style.buttons.close_tab_active_color = palette.accent;
        style.buttons.close_tab_bg_fill = palette.panel_alt;
        style.buttons.add_tab_color = palette.text_primary;
        style.buttons.add_tab_active_color = palette.accent;
        style.buttons.add_tab_bg_fill = palette.panel_alt;
        style.buttons.add_tab_border_color = palette.toolbar_outline;

        style.separator.color_idle = palette.toolbar_outline;
        style.separator.color_hovered = palette.accent;
        style.separator.color_dragged = palette.accent_alt;

        style.overlay.selection_color = palette.accent_soft.gamma_multiply(0.6);
        style.overlay.button_color = palette.accent;
        style.overlay.button_border_stroke = Stroke::new(1.0, palette.toolbar_outline);
        style.overlay.hovered_leaf_highlight.color = palette.accent_soft.gamma_multiply(0.25);
        style.overlay.hovered_leaf_highlight.rounding = Rounding::same(12.0);
        style.overlay.hovered_leaf_highlight.stroke = Stroke::new(1.0, palette.accent);

        style
    }

    fn send_command(&mut self, command: EngineCommand) {
        if let Err(command) = self.command_queue.try_send(command) {
            self.status_message = Some(format!("Command queue full: {command:?}"));
        }
    }

    fn process_events(&mut self) {
        for event in self.event_bus.drain() {
            match event {
                AppEvent::Transport(event) => self.handle_transport_event(event),
                AppEvent::SetTempo(tempo) => {
                    self.tempo = tempo.clamp(20.0, 400.0);
                    self.send_command(EngineCommand::SetTempo(self.tempo));
                }
                AppEvent::SetTimeSignature(sig) => {
                    self.time_signature = sig;
                }
                AppEvent::ToggleMetronome => {
                    self.metronome = !self.metronome;
                }
                AppEvent::TogglePatternMode => {
                    self.pattern_mode = !self.pattern_mode;
                }
                AppEvent::Layout(event) => match event {
                    LayoutEvent::ToggleBrowser => {
                        let visible = !self.browser.is_visible();
                        self.browser.set_visible(visible);
                        self.layout.set_browser_visible(visible);
                    }
                    LayoutEvent::ResetWorkspace => {
                        self.dock_state = build_default_workspace();
                        self.layout.store_dock(&self.dock_state);
                        self.browser.set_visible(true);
                        self.browser.set_width(260.0);
                        self.layout.set_browser_visible(true);
                        self.layout.set_browser_width(260.0);
                    }
                },
                AppEvent::OpenFile(path) => {
                    self.status_message = Some(format!("Opened {}", path.display()));
                }
                AppEvent::SaveProject => {
                    self.status_message = Some("Project saved".into());
                }
                AppEvent::OpenAudioSettings => {
                    let config = self.engine_runner.config().clone();
                    let runtime = self.engine_runner.runtime_options().clone();
                    self.audio_settings.open(&config, &runtime);
                }
                AppEvent::RequestRepaint => {}
            }
        }
    }

    fn handle_transport_event(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Play => {
                self.transport_state = TransportState::Playing;
                self.send_command(EngineCommand::SetTransport(TransportState::Playing));
            }
            TransportEvent::Stop => {
                self.transport_state = TransportState::Stopped;
                self.send_command(EngineCommand::SetTransport(TransportState::Stopped));
            }
            TransportEvent::Record(armed) => {
                self.transport_state = if armed {
                    TransportState::Recording
                } else {
                    TransportState::Playing
                };
                self.send_command(EngineCommand::SetTransport(self.transport_state));
            }
        }
    }

    fn update_engine_context(&mut self) {
        if self.last_engine_update.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_engine_update = Instant::now();
        self.transport_clock =
            TransportClock::from_beats(self.playlist.playhead_position(), self.time_signature);
        let mut ctx = self.engine_context.lock();
        ctx.tempo = self.tempo;
        ctx.time_signature = self.time_signature;
        ctx.transport = self.transport_state;
        ctx.cpu_usage = self.mixer.cpu_estimate();
        ctx.clock = self.transport_clock;
        ctx.master_meter = self.mixer.master_meter();
    }
}

struct WorkspaceTabViewer<'a> {
    palette: &'a HarmoniqPalette,
    event_bus: &'a EventBus,
    channel_rack: &'a mut ChannelRackPane,
    piano_roll: &'a mut PianoRollPane,
    mixer: &'a mut MixerPane,
    playlist: &'a mut PlaylistPane,
    transport_state: TransportState,
    transport_clock: TransportClock,
}

impl<'a> TabViewer for WorkspaceTabViewer<'a> {
    type Tab = WorkspacePane;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            WorkspacePane::ChannelRack => self.channel_rack.ui(ui, self.palette, self.event_bus),
            WorkspacePane::PianoRoll => self.piano_roll.ui(ui, self.palette, self.event_bus),
            WorkspacePane::Mixer => self.mixer.ui(ui, self.palette, self.event_bus),
            WorkspacePane::Playlist => self.playlist.ui(
                ui,
                self.palette,
                self.event_bus,
                self.transport_state,
                self.transport_clock,
            ),
        }
    }
}

impl App for HarmoniqStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events();
        self.update_engine_context();

        let palette = self.theme.palette().clone();

        egui::TopBottomPanel::top("menu_bar")
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .outer_margin(Margin::symmetric(8.0, 4.0)),
            )
            .show(ctx, |ui| {
                self.menu
                    .ui(ui, &palette, &self.event_bus, self.browser.is_visible());
            });

        egui::TopBottomPanel::top("transport_bar")
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .outer_margin(Margin::symmetric(8.0, 0.0)),
            )
            .show(ctx, |ui| {
                let snapshot = TransportSnapshot {
                    tempo: self.tempo,
                    time_signature: self.time_signature,
                    transport: self.transport_state,
                    clock: self.transport_clock,
                    metronome: self.metronome,
                    pattern_mode: self.pattern_mode,
                };
                self.transport_bar
                    .ui(ui, &palette, &self.icons, &self.event_bus, snapshot);
            });

        if self.browser.is_visible() {
            let panel = egui::SidePanel::left("browser_panel")
                .resizable(true)
                .default_width(self.browser.width())
                .frame(
                    egui::Frame::none()
                        .fill(palette.panel)
                        .stroke(Stroke::new(1.0, palette.toolbar_outline))
                        .inner_margin(Margin::symmetric(12.0, 10.0))
                        .rounding(Rounding::same(16.0)),
                )
                .show(ctx, |ui| {
                    self.browser.ui(ui, &palette, &self.event_bus);
                });
            self.browser.set_width(panel.response.rect.width());
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .rounding(Rounding::same(18.0)),
            )
            .show(ctx, |ui| {
                let dock_style = self.dock_style.clone();
                let mut tab_viewer = WorkspaceTabViewer {
                    palette: &palette,
                    event_bus: &self.event_bus,
                    channel_rack: &mut self.channel_rack,
                    piano_roll: &mut self.piano_roll,
                    mixer: &mut self.mixer,
                    playlist: &mut self.playlist,
                    transport_state: self.transport_state,
                    transport_clock: self.transport_clock,
                };
                DockArea::new(&mut self.dock_state)
                    .style(dock_style)
                    .show_inside(ui, &mut tab_viewer);
            });

        let active_audio_summary =
            self.engine_runner
                .realtime()
                .map(|runtime| ActiveAudioSummary {
                    backend: runtime.backend(),
                    device_name: runtime.device_name().to_string(),
                    host_label: runtime.host_label().map(|label| label.to_string()),
                });
        let last_runtime_error = self
            .engine_runner
            .last_runtime_error()
            .map(|err| err.to_string());

        if let Some(AudioSettingsAction::Apply { config, runtime }) = self.audio_settings.ui(
            ctx,
            &palette,
            active_audio_summary.as_ref(),
            last_runtime_error.as_deref(),
        ) {
            let result = self
                .engine_runner
                .reconfigure(config.clone(), runtime.clone());
            self.audio_settings
                .on_apply_result(result, &config, &runtime);
        }

        if let Some(feedback) = self.audio_settings.take_status_message() {
            if feedback.is_error() {
                warn!("{}", feedback.message());
            }
            self.status_message = Some(feedback.message().to_string());
        }

        if let Some(message) = &self.status_message {
            egui::Area::new(Id::new("status_message"))
                .anchor(Align2::LEFT_BOTTOM, [16.0, -16.0])
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(palette.panel)
                        .stroke(Stroke::new(1.0, palette.toolbar_outline))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::symmetric(12.0, 10.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(message).color(palette.text_primary));
                        });
                });
        }

        self.layout.set_browser_width(self.browser.width());
        self.layout.store_dock(&self.dock_state);
        self.layout.maybe_save();

        if ctx.input(|i| i.pointer.button_released(PointerButton::Primary)) {
            ctx.set_cursor_icon(CursorIcon::Default);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.layout.flush();
    }
}
