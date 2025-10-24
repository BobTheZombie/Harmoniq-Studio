use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use flacenc::component::BitRepr;
use flacenc::error::Verify;

use anyhow::{anyhow, Context};
use clap::{Parser, ValueEnum};
use eframe::egui::{
    self, Align2, CursorIcon, Id, Margin, PointerButton, RichText, Rounding, Stroke,
    ViewportCommand,
};
use eframe::{App, CreationContext, NativeOptions};
use egui_dock::{DockArea, DockState, Style as DockStyle, TabViewer};
use egui_extras::{image::load_svg_bytes, install_image_loaders};
use harmoniq_engine::{
    automation::{AutomationCommand, CurveShape, ParameterSpec},
    transport::Transport as EngineTransport,
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine,
    PluginId, TransportState,
};
use harmoniq_plugins::{GainPlugin, NoisePlugin, SineSynth};
use harmoniq_ui::{HarmoniqPalette, HarmoniqTheme};
use hound::{SampleFormat, WavSpec, WavWriter};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;
use winit::keyboard::{KeyCode, ModifiersState};

mod audio;
mod config;
mod midi;
mod ui;

use audio::{
    available_backends, describe_layout, AudioBackend, AudioRuntimeOptions, RealtimeAudio,
    SoundTestSample,
};
use config::qwerty::{QwertyConfig, VelocityCurveSetting};
use midi::{list_midi_inputs, MidiInputDevice, QwertyKeyboardInput};
use ui::{
    audio_settings::{ActiveAudioSummary, AudioSettingsAction, AudioSettingsPanel},
    browser::BrowserPane,
    channel_rack::ChannelRackPane,
    command_dispatch::{command_channel, CommandDispatcher, CommandHandler, CommandSender},
    commands::{
        Command, EditCommand, FileCommand, HelpCommand, InsertCommand, MidiCommand, OptionsCommand,
        TrackCommand, TransportCommand, ViewCommand,
    },
    config::RecentProjects,
    console::{ConsolePane, LogLevel},
    event_bus::{AppEvent, EventBus, LayoutEvent, TransportEvent},
    focus::InputFocus,
    inspector::InspectorPane,
    layout::LayoutState,
    menu_bar::{MenuBarSnapshot, MenuBarState},
    mixer::MixerPane,
    piano_roll::PianoRollPane,
    playlist::PlaylistPane,
    shortcuts::ShortcutMap,
    transport::{TransportBar, TransportSnapshot},
    workspace::{build_default_workspace, WorkspacePane},
};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum QwertyCurveArg {
    Linear,
    Soft,
    Hard,
    Fixed,
}

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

    /// Initial tempo used for transport and offline bouncing (beats per minute)
    #[arg(long = "bpm", default_value_t = 120.0)]
    bpm: f32,

    /// Initial time signature used for the transport (e.g. "4/4")
    #[arg(long = "sig", default_value = "4/4")]
    signature: String,

    /// Enable Harmoniq Ultra runtime profile heuristics (prefers OpenASIO when available)
    #[arg(long, default_value_t = false)]
    ultra: bool,

    /// Preferred realtime audio backend
    #[arg(long, default_value_t = AudioBackend::Auto)]
    audio_backend: AudioBackend,

    /// Path to the OpenASIO driver (.so)
    #[cfg(feature = "openasio")]
    #[arg(long)]
    openasio_driver: Option<String>,

    /// Device name to open when using OpenASIO
    #[cfg(feature = "openasio")]
    #[arg(long)]
    openasio_device: Option<String>,

    /// Override the OpenASIO sample rate when using the ultra runtime
    #[cfg(feature = "openasio")]
    #[arg(long)]
    openasio_sr: Option<u32>,

    /// Override the OpenASIO buffer size in frames when using the ultra runtime
    #[cfg(feature = "openasio")]
    #[arg(long)]
    openasio_buffer: Option<u32>,

    /// Disable realtime audio streaming
    #[arg(long, default_value_t = false)]
    disable_audio: bool,

    /// Name of the MIDI controller or input port to connect
    #[arg(long)]
    midi_input: Option<String>,

    /// Apply automation in the demo graph before playback (e.g. --auto gain=-6@1.0s)
    #[arg(long = "auto")]
    auto: Vec<String>,

    /// List available realtime audio backends
    #[arg(long, default_value_t = false)]
    list_audio_backends: bool,

    /// List available MIDI input ports
    #[arg(long, default_value_t = false)]
    list_midi_inputs: bool,

    /// Force-enable the QWERTY keyboard input device
    #[arg(long, default_value_t = false)]
    qwerty: bool,

    /// Disable the QWERTY keyboard input device
    #[arg(long, default_value_t = false)]
    no_qwerty: bool,

    /// Override the default octave for the QWERTY keyboard
    #[arg(long)]
    qwerty_octave: Option<i8>,

    /// Select the velocity curve for the QWERTY keyboard
    #[arg(long, value_enum)]
    qwerty_curve: Option<QwertyCurveArg>,

    /// Fixed velocity value (0-127) used when the velocity curve is "fixed"
    #[arg(long)]
    qwerty_velocity: Option<u8>,

    /// MIDI channel (1-16) used for the QWERTY keyboard
    #[arg(long)]
    qwerty_channel: Option<u8>,
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
) -> anyhow::Result<PluginId> {
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
    engine
        .register_automation_parameter(
            gain,
            ParameterSpec::new(0, "Gain", 0.0, 2.0, graph_config.gain),
        )
        .context("register automation parameter")?;

    let mut graph_builder = GraphBuilder::new();
    let sine_node = graph_builder.add_node(sine);
    graph_builder.connect_to_mixer(sine_node, graph_config.sine_mix)?;
    let noise_node = graph_builder.add_node(noise);
    graph_builder.connect_to_mixer(noise_node, graph_config.noise_mix)?;
    let gain_node = graph_builder.add_node(gain);
    graph_builder.connect_to_mixer(gain_node, 1.0)?;

    engine.replace_graph(graph_builder.build())?;
    Ok(gain)
}

#[derive(Debug, Clone)]
struct AutoSpec {
    name: String,
    value: f32,
    value_in_db: bool,
    time_seconds: f32,
}

fn parse_auto_spec(spec: &str) -> anyhow::Result<AutoSpec> {
    let (name_value, time_part) = spec
        .split_once('@')
        .ok_or_else(|| anyhow!("automation spec must contain '@': {spec}"))?;
    let (name, value_part) = name_value
        .split_once('=')
        .ok_or_else(|| anyhow!("automation spec must contain '=': {spec}"))?;

    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("automation parameter name cannot be empty");
    }

    let mut value_fragment = value_part.trim();
    let mut value_in_db = false;
    if value_fragment.to_ascii_lowercase().ends_with("db") {
        value_in_db = true;
        value_fragment = value_fragment[..value_fragment.len() - 2].trim();
    }
    let value = value_fragment
        .parse::<f32>()
        .map_err(|err| anyhow!("invalid automation value '{value_fragment}': {err}"))?;

    let mut time_fragment = time_part.trim();
    let mut factor = 1.0;
    if time_fragment.to_ascii_lowercase().ends_with("ms") {
        time_fragment = time_fragment[..time_fragment.len() - 2].trim();
        factor = 0.001;
    } else if time_fragment.ends_with('s') || time_fragment.ends_with('S') {
        time_fragment = time_fragment[..time_fragment.len() - 1].trim();
    }

    let seconds = time_fragment
        .parse::<f32>()
        .map_err(|err| anyhow!("invalid automation time '{time_fragment}': {err}"))?
        * factor;
    if seconds < 0.0 {
        anyhow::bail!("automation time cannot be negative");
    }

    Ok(AutoSpec {
        name: name.to_string(),
        value,
        value_in_db,
        time_seconds: seconds,
    })
}

fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn apply_cli_automation(
    engine: &HarmoniqEngine,
    plugin_id: PluginId,
    sample_rate: f32,
    specs: &[String],
) -> anyhow::Result<()> {
    if specs.is_empty() {
        return Ok(());
    }

    let sender = match engine.automation_sender(plugin_id) {
        Some(sender) => sender,
        None => return Ok(()),
    };

    for raw in specs {
        let parsed = parse_auto_spec(raw)?;
        let parameter_index = engine
            .automation_parameter_index(plugin_id, &parsed.name)
            .ok_or_else(|| anyhow!("unknown automation parameter '{}'", parsed.name))?;
        let parameter_spec = engine
            .automation_parameter_spec(plugin_id, parameter_index)
            .ok_or_else(|| anyhow!("missing automation metadata for '{}'", parsed.name))?;

        let mut value = parsed.value;
        if parsed.value_in_db || parsed.name.eq_ignore_ascii_case("gain") {
            value = db_to_linear(value);
        }
        let value = parameter_spec.clamp(value);
        let samples = (parsed.time_seconds.max(0.0) * sample_rate.max(0.0)).round() as u64;

        sender
            .send(AutomationCommand::DrawCurve {
                parameter: parameter_index,
                sample: samples,
                value,
                shape: CurveShape::Step,
            })
            .map_err(|_| anyhow!("automation queue full for parameter '{}'", parsed.name))?;
    }

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

impl FromStr for TimeSignature {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('/');
        let numerator = parts
            .next()
            .ok_or_else(|| anyhow!("missing numerator"))?
            .trim()
            .parse::<u32>()?;
        let denominator = parts
            .next()
            .ok_or_else(|| anyhow!("missing denominator"))?
            .trim()
            .parse::<u32>()?;
        if parts.next().is_some() {
            anyhow::bail!("invalid time signature format");
        }
        if numerator == 0 || denominator == 0 {
            anyhow::bail!("time signature components must be non-zero");
        }
        Ok(Self {
            numerator,
            denominator,
        })
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
    bpm: f32,
    beats: f32,
    automation: &[String],
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

    let total_seconds = (60.0 / bpm.max(1.0)) * beats;
    let total_frames = (total_seconds * config.sample_rate) as usize;
    let channels = config.layout.channels() as usize;

    let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
    let gain_id = configure_demo_graph(&mut engine, graph_config)?;
    apply_cli_automation(&engine, gain_id, config.sample_rate, automation)
        .context("failed to apply automation")?;
    engine.set_transport(TransportState::Playing);

    let mut buffer = AudioBuffer::from_config(&config);
    let mut frames_written = 0usize;
    let mut samples: Vec<i16> = Vec::with_capacity(total_frames * channels);

    while frames_written < total_frames {
        engine.process_block(&mut buffer)?;
        let block_len = buffer.len();
        let channel_count = buffer.channel_count();

        for frame in 0..block_len {
            if frames_written >= total_frames {
                break;
            }
            for channel in 0..channels {
                let sample = if channel < channel_count {
                    buffer.channel(channel)[frame]
                } else {
                    0.0
                }
                .clamp(-1.0, 1.0);
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
    let initial_signature = args
        .signature
        .parse::<TimeSignature>()
        .context("failed to parse --sig time signature")?;
    let initial_bpm = args.bpm;
    let ultra_mode = args.ultra || cfg!(feature = "ultra");
    harmoniq_engine::rt::enable_ftz_daz();
    if ultra_mode {
        if let Err(err) = harmoniq_engine::rt::mlock_process() {
            warn!(?err, "mlockall failed; continuing without locked memory");
        }
        if let Err(err) = harmoniq_engine::rt::pin_current_thread(0) {
            warn!(
                ?err,
                "failed to pin main thread; realtime threads will attempt pinning separately"
            );
        }
    }

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

    let available_ports = list_midi_inputs().unwrap_or_else(|err| {
        warn!("Unable to enumerate MIDI inputs: {err}");
        Vec::new()
    });

    let mut qwerty_config = QwertyConfig::load();
    if args.no_qwerty {
        qwerty_config.file.enabled = false;
    } else if args.qwerty {
        qwerty_config.file.enabled = true;
    } else if available_ports.is_empty() {
        qwerty_config.file.enabled = true;
    }

    if let Some(octave) = args.qwerty_octave {
        qwerty_config.file.octave = octave.clamp(1, 7);
    }

    if let Some(channel) = args.qwerty_channel {
        qwerty_config.file.channel = channel.clamp(1, 16);
    }

    if let Some(curve) = args.qwerty_curve {
        qwerty_config.file.velocity_curve = match curve {
            QwertyCurveArg::Linear => VelocityCurveSetting::Linear,
            QwertyCurveArg::Soft => VelocityCurveSetting::Soft,
            QwertyCurveArg::Hard => VelocityCurveSetting::Hard,
            QwertyCurveArg::Fixed => VelocityCurveSetting::Fixed,
        };
    }

    if let Some(velocity) = args.qwerty_velocity {
        qwerty_config.file.fixed_velocity = velocity.min(127);
        qwerty_config.file.velocity_curve = VelocityCurveSetting::Fixed;
    }

    let mut runtime_options = AudioRuntimeOptions::new(
        args.audio_backend,
        args.midi_input.clone(),
        !args.disable_audio,
    );

    #[cfg(feature = "openasio")]
    {
        runtime_options.openasio_driver = args.openasio_driver.clone();
        runtime_options.openasio_device = args.openasio_device.clone();
        runtime_options.openasio_sample_rate = args.openasio_sr;
        runtime_options.openasio_buffer_frames = args.openasio_buffer;
    }

    if ultra_mode && runtime_options.backend() == AudioBackend::Auto {
        #[cfg(feature = "openasio")]
        {
            runtime_options.set_backend(AudioBackend::OpenAsio);
        }
        #[cfg(not(feature = "openasio"))]
        {
            runtime_options.set_backend(AudioBackend::Harmoniq);
        }
    }

    #[cfg(feature = "openasio")]
    let selected_backend = runtime_options.backend();

    let sample_rate = {
        #[cfg(feature = "openasio")]
        {
            if matches!(
                selected_backend,
                AudioBackend::OpenAsio | AudioBackend::Harmoniq
            ) {
                runtime_options
                    .openasio_sample_rate
                    .map(|sr| sr as f32)
                    .unwrap_or(args.sample_rate)
            } else {
                args.sample_rate
            }
        }
        #[cfg(not(feature = "openasio"))]
        {
            args.sample_rate
        }
    };

    let block_size = {
        #[cfg(feature = "openasio")]
        {
            if matches!(
                selected_backend,
                AudioBackend::OpenAsio | AudioBackend::Harmoniq
            ) {
                runtime_options
                    .openasio_buffer_frames
                    .map(|frames| frames as usize)
                    .unwrap_or(args.block_size)
            } else {
                args.block_size
            }
        }
        #[cfg(not(feature = "openasio"))]
        {
            args.block_size
        }
    };

    if let Some(path) = args.bounce.clone() {
        let config = BufferConfig::new(sample_rate, block_size, ChannelLayout::Stereo);
        let bounced_path = offline_bounce_to_file(
            path,
            config,
            &DemoGraphConfig::headless_default(),
            initial_bpm,
            args.bounce_beats,
            &args.auto,
        )?;
        println!("Offline bounce written to {}", bounced_path.display());
        return Ok(());
    }

    let ui_requested = !args.headless;
    let ui_supported = ui_requested && environment_supports_native_ui();

    if ui_requested && !ui_supported {
        eprintln!(
            "No graphical display detected – falling back to --headless mode. Set the DISPLAY or WAYLAND_DISPLAY environment variables to use the native UI."
        );
    }

    if args.headless || !ui_supported {
        run_headless(
            &args,
            runtime_options,
            sample_rate,
            block_size,
            initial_signature,
            initial_bpm,
        )
    } else {
        run_ui(
            runtime_options,
            sample_rate,
            block_size,
            initial_signature,
            initial_bpm,
            qwerty_config,
        )
    }
}

fn run_headless(
    args: &Cli,
    runtime: AudioRuntimeOptions,
    sample_rate: f32,
    block_size: usize,
    _time_signature: TimeSignature,
    bpm: f32,
) -> anyhow::Result<()> {
    let config = BufferConfig::new(sample_rate, block_size, ChannelLayout::Stereo);
    let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
    let gain_id = configure_demo_graph(&mut engine, &DemoGraphConfig::headless_default())?;
    apply_cli_automation(&engine, gain_id, sample_rate, &args.auto)
        .context("failed to apply automation")?;
    engine.set_transport(TransportState::Playing);
    engine.execute_command(EngineCommand::SetTempo(bpm))?;

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
                run_headless_offline_preview(&engine, &config, bpm)?;
            }
        }
    } else {
        run_headless_offline_preview(&engine, &config, bpm)?;
    }

    Ok(())
}

fn environment_supports_native_ui() -> bool {
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        true
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "windows",
        target_os = "macos"
    )))]
    {
        false
    }
}

fn run_headless_offline_preview(
    engine: &Arc<Mutex<HarmoniqEngine>>,
    config: &BufferConfig,
    bpm: f32,
) -> anyhow::Result<()> {
    let mut buffer = AudioBuffer::from_config(config);
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
        bpm
    );

    Ok(())
}

fn run_ui(
    runtime: AudioRuntimeOptions,
    sample_rate: f32,
    block_size: usize,
    time_signature: TimeSignature,
    bpm: f32,
    qwerty_config: QwertyConfig,
) -> anyhow::Result<()> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    let config = BufferConfig::new(sample_rate, block_size, ChannelLayout::Stereo);
    let config_for_app = config.clone();
    let tempo_for_app = bpm;
    let signature_for_app = time_signature;
    let runtime_for_app = runtime.clone();
    let qwerty_config_for_app = qwerty_config.clone();

    eframe::run_native(
        "Harmoniq Studio",
        native_options,
        Box::new(move |cc| {
            install_image_loaders(&cc.egui_ctx);
            let config = config_for_app.clone();
            let app = match HarmoniqStudioApp::new(
                config,
                tempo_for_app,
                signature_for_app,
                runtime_for_app.clone(),
                qwerty_config_for_app.clone(),
                cc,
            ) {
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
        let (transport_handle, playing_state) = {
            let engine_guard = engine.lock();
            (
                engine_guard.transport_metrics(),
                matches!(
                    engine_guard.transport(),
                    TransportState::Playing | TransportState::Recording
                ),
            )
        };
        transport_handle
            .sample_rate
            .store(config.sample_rate.round() as u64, AtomicOrdering::Relaxed);
        transport_handle
            .sample_pos
            .store(0, AtomicOrdering::Relaxed);
        transport_handle
            .playing
            .store(playing_state, AtomicOrdering::Relaxed);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let handle = std::thread::spawn(move || {
            let mut buffer = AudioBuffer::from_config(&config);
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
    inspector: InspectorPane,
    console: ConsolePane,
    audio_settings: AudioSettingsPanel,
    sound_test: SoundTestSample,
    event_bus: EventBus,
    input_focus: InputFocus,
    shortcuts: ShortcutMap,
    command_sender: CommandSender,
    command_dispatcher: CommandDispatcher,
    recent_projects: RecentProjects,
    midi_inputs: Vec<String>,
    selected_midi_input: Option<String>,
    midi_channel: u8,
    engine_runner: EngineRunner,
    command_queue: harmoniq_engine::EngineCommandQueue,
    qwerty: Option<QwertyKeyboardInput>,
    qwerty_config: QwertyConfig,
    transport: Arc<EngineTransport>,
    engine_context: Arc<Mutex<EngineContext>>,
    tempo: f32,
    time_signature: TimeSignature,
    transport_state: TransportState,
    transport_clock: TransportClock,
    metronome: bool,
    pattern_mode: bool,
    transport_loop_enabled: bool,
    record_armed: bool,
    last_engine_update: Instant,
    status_message: Option<String>,
    browser_hidden: bool,
    mixer_hidden: bool,
    piano_roll_hidden: bool,
    fullscreen: bool,
    fullscreen_dirty: bool,
}

impl HarmoniqStudioApp {
    fn new(
        config: BufferConfig,
        initial_tempo: f32,
        initial_time_signature: TimeSignature,
        runtime: AudioRuntimeOptions,
        qwerty_config: QwertyConfig,
        cc: &CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        let theme = HarmoniqTheme::init(&cc.egui_ctx);
        let dock_style = Self::create_dock_style(cc.egui_ctx.style().as_ref(), theme.palette());
        let icons = AppIcons::load(&cc.egui_ctx)?;

        let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
        let graph_config = DemoGraphConfig::ui_default();
        let _ = configure_demo_graph(&mut engine, &graph_config)?;
        engine.set_transport(TransportState::Stopped);
        engine.execute_command(EngineCommand::SetTempo(initial_tempo))?;

        let command_queue = engine.command_queue();
        let transport = engine.transport_metrics();
        let engine = Arc::new(Mutex::new(engine));
        let engine_runner = EngineRunner::start(
            Arc::clone(&engine),
            config.clone(),
            command_queue.clone(),
            runtime,
        )?;

        let qwerty = Some(QwertyKeyboardInput::new(qwerty_config.file.clone()));

        let engine_context = Arc::new(Mutex::new(EngineContext::new(
            initial_tempo,
            initial_time_signature,
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
        let transport_bar = TransportBar::new(initial_tempo, initial_time_signature);
        let browser_visible = layout.browser_visible();
        let browser_width = layout.browser_width();
        let browser = BrowserPane::new(resources_root, browser_visible, browser_width);
        let channel_rack = ChannelRackPane::default();
        let piano_roll = PianoRollPane::default();
        let mixer = MixerPane::default();
        let playlist = PlaylistPane::default();
        let inspector = InspectorPane::new();
        let console = ConsolePane::default();
        let input_focus = InputFocus::default();
        let audio_settings =
            AudioSettingsPanel::new(engine_runner.config(), engine_runner.runtime_options());
        let sound_test = SoundTestSample::load().context("failed to load sound test sample")?;
        let shortcuts = ShortcutMap::load();
        let (command_sender, command_receiver) = command_channel(256);
        let recent_projects = RecentProjects::load();
        let midi_inputs = list_midi_inputs().unwrap_or_else(|err| {
            warn!("Unable to enumerate MIDI inputs: {err}");
            Vec::new()
        });
        let selected_midi_input = midi_inputs.first().cloned();
        let command_dispatcher = CommandDispatcher::new(command_receiver);

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
            inspector,
            console,
            audio_settings,
            sound_test,
            event_bus,
            input_focus,
            shortcuts,
            command_sender,
            command_dispatcher,
            recent_projects,
            midi_inputs,
            selected_midi_input,
            midi_channel: 1,
            engine_runner,
            command_queue,
            qwerty,
            qwerty_config,
            transport,
            engine_context,
            tempo: initial_tempo,
            time_signature: initial_time_signature,
            transport_state: TransportState::Stopped,
            transport_clock: TransportClock::default(),
            metronome: false,
            pattern_mode: true,
            transport_loop_enabled: false,
            record_armed: false,
            last_engine_update: Instant::now(),
            status_message,
            browser_hidden: !browser_visible,
            mixer_hidden: false,
            piano_roll_hidden: false,
            fullscreen: false,
            fullscreen_dirty: false,
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

    fn play_sound_test(&mut self) {
        let sample_rate = self.engine_runner.config().sample_rate;
        let clip = self.sound_test.prepare_clip(sample_rate);
        match self
            .command_queue
            .try_send(EngineCommand::PlaySoundTest(clip))
        {
            Ok(()) => {
                self.status_message = Some("Playing sound test".to_string());
            }
            Err(command) => {
                self.status_message = Some(format!("Command queue full: {command:?}"));
            }
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
                        self.browser_hidden = !self.browser_hidden;
                        self.browser.set_visible(!self.browser_hidden);
                        self.layout.set_browser_visible(!self.browser_hidden);
                        let state = if self.browser_hidden {
                            "hidden"
                        } else {
                            "shown"
                        };
                        self.console.log(LogLevel::Info, format!("Browser {state}"));
                    }
                    LayoutEvent::ResetWorkspace => {
                        self.dock_state = build_default_workspace();
                        self.layout.store_dock(&self.dock_state);
                        self.browser_hidden = false;
                        self.browser.set_visible(true);
                        self.layout.set_browser_visible(true);
                        self.console.log(LogLevel::Info, "Workspace layout reset");
                    }
                },
                AppEvent::OpenFile(path) => {
                    self.status_message = Some(format!("Opened {}", path.display()));
                    self.console
                        .log(LogLevel::Info, format!("Opened {}", path.display()));
                    self.recent_projects.add(path);
                    if let Err(err) = self.recent_projects.save() {
                        warn!("Failed to persist recent projects: {err}");
                    }
                }
                AppEvent::SaveProject => {
                    self.status_message = Some("Project saved".into());
                    self.console.log(LogLevel::Info, "Project saved");
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

    fn process_qwerty_keyboard(&mut self, ctx: &egui::Context) {
        let Some(device) = self.qwerty.as_mut() else {
            return;
        };

        if !device.enabled() {
            return;
        }

        if ctx.wants_keyboard_input() {
            device.panic(Instant::now());
            return;
        }

        let mut lost_focus = false;
        ctx.input(|input| {
            if !input.focused {
                lost_focus = true;
            }
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } = event
                {
                    if *repeat {
                        continue;
                    }
                    if let Some(code) = map_virtual_key(*key) {
                        let modifiers_state = map_modifiers_state(*modifiers);
                        device.push_key_event(code, *pressed, modifiers_state, Instant::now());
                    }
                }
            }
        });

        if lost_focus {
            device.panic(Instant::now());
        }

        let mut collected = Vec::new();
        device.drain_events(&mut |event, _timestamp| {
            collected.push(event);
        });

        if !collected.is_empty() {
            let command = EngineCommand::SubmitMidi(collected);
            if let Err(command) = self.command_queue.try_send(command) {
                self.status_message = Some(format!("Command queue full: {command:?}"));
            }
        }
    }

    fn handle_transport_event(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Play => {
                self.transport_state = TransportState::Playing;
                self.transport.sample_pos.store(0, AtomicOrdering::Relaxed);
                self.send_command(EngineCommand::SetTransport(TransportState::Playing));
            }
            TransportEvent::Stop => {
                self.transport_state = TransportState::Stopped;
                self.transport.sample_pos.store(0, AtomicOrdering::Relaxed);
                self.send_command(EngineCommand::SetTransport(TransportState::Stopped));
            }
            TransportEvent::Record(armed) => {
                self.record_armed = armed;
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
        let sample_rate = self
            .transport
            .sample_rate
            .load(AtomicOrdering::Relaxed)
            .max(1);
        let sample_pos = self.transport.sample_pos.load(AtomicOrdering::Relaxed);
        let is_playing = self.transport.playing.load(AtomicOrdering::Relaxed);
        let seconds = sample_pos as f64 / sample_rate as f64;
        let beats = (seconds * (self.tempo.max(1.0) as f64 / 60.0)) as f32;
        self.transport_clock = TransportClock::from_beats(beats, self.time_signature);
        self.playlist.set_playhead(beats, is_playing);

        if self.last_engine_update.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_engine_update = Instant::now();
        let mut ctx = self.engine_context.lock();
        ctx.tempo = self.tempo;
        ctx.time_signature = self.time_signature;
        ctx.transport = self.transport_state;
        ctx.cpu_usage = self.mixer.cpu_estimate();
        ctx.clock = self.transport_clock;
        ctx.master_meter = self.mixer.master_meter();
    }
}

fn map_virtual_key(key: egui::Key) -> Option<KeyCode> {
    use egui::Key;
    match key {
        Key::Q => Some(KeyCode::KeyQ),
        Key::W => Some(KeyCode::KeyW),
        Key::E => Some(KeyCode::KeyE),
        Key::R => Some(KeyCode::KeyR),
        Key::T => Some(KeyCode::KeyT),
        Key::Y => Some(KeyCode::KeyY),
        Key::U => Some(KeyCode::KeyU),
        Key::I => Some(KeyCode::KeyI),
        Key::O => Some(KeyCode::KeyO),
        Key::P => Some(KeyCode::KeyP),
        Key::A => Some(KeyCode::KeyA),
        Key::S => Some(KeyCode::KeyS),
        Key::D => Some(KeyCode::KeyD),
        Key::F => Some(KeyCode::KeyF),
        Key::G => Some(KeyCode::KeyG),
        Key::H => Some(KeyCode::KeyH),
        Key::J => Some(KeyCode::KeyJ),
        Key::K => Some(KeyCode::KeyK),
        Key::L => Some(KeyCode::KeyL),
        Key::Z => Some(KeyCode::KeyZ),
        Key::X => Some(KeyCode::KeyX),
        Key::C => Some(KeyCode::KeyC),
        Key::V => Some(KeyCode::KeyV),
        Key::B => Some(KeyCode::KeyB),
        Key::N => Some(KeyCode::KeyN),
        Key::M => Some(KeyCode::KeyM),
        Key::Comma => Some(KeyCode::Comma),
        Key::Slash => Some(KeyCode::Slash),
        Key::OpenBracket => Some(KeyCode::BracketLeft),
        Key::Num0 => Some(KeyCode::Digit0),
        Key::Num1 => Some(KeyCode::Digit1),
        Key::Num2 => Some(KeyCode::Digit2),
        Key::Num3 => Some(KeyCode::Digit3),
        Key::Num4 => Some(KeyCode::Digit4),
        Key::Num5 => Some(KeyCode::Digit5),
        Key::Num6 => Some(KeyCode::Digit6),
        Key::Num7 => Some(KeyCode::Digit7),
        Key::Num8 => Some(KeyCode::Digit8),
        Key::Num9 => Some(KeyCode::Digit9),
        Key::Space => Some(KeyCode::Space),
        Key::Escape => Some(KeyCode::Escape),
        _ => None,
    }
}

fn map_modifiers_state(modifiers: egui::Modifiers) -> ModifiersState {
    let mut state = ModifiersState::empty();
    state.set(ModifiersState::SHIFT, modifiers.shift);
    state.set(ModifiersState::CONTROL, modifiers.ctrl);
    state.set(ModifiersState::ALT, modifiers.alt);
    state.set(
        ModifiersState::SUPER,
        modifiers.mac_cmd || modifiers.command,
    );
    state
}

impl CommandHandler for HarmoniqStudioApp {
    fn handle_command(&mut self, command: Command) {
        match command {
            Command::File(cmd) => match cmd {
                FileCommand::New => {
                    self.status_message = Some("Starting new project".into());
                    self.console.log(LogLevel::Info, "New project started");
                }
                FileCommand::Open => {
                    self.status_message = Some("Open project…".into());
                    self.console
                        .log(LogLevel::Info, "Open project dialog requested");
                }
                FileCommand::OpenRecent(path) => {
                    self.status_message = Some(format!("Opened {}", path.display()));
                    self.console
                        .log(LogLevel::Info, format!("Opened {}", path.display()));
                    self.recent_projects.add(path.clone());
                    if let Err(err) = self.recent_projects.save() {
                        warn!("Failed to persist recent projects: {err}");
                    }
                }
                FileCommand::Save => {
                    self.status_message = Some("Project saved".into());
                    self.console.log(LogLevel::Info, "Project saved");
                }
                FileCommand::SaveAs => {
                    self.status_message = Some("Save project as…".into());
                    self.console.log(LogLevel::Info, "Save As dialog requested");
                }
                FileCommand::Export => {
                    self.status_message = Some("Export/Render not implemented".into());
                    self.console
                        .log(LogLevel::Warning, "Export/Render flow not implemented yet");
                }
                FileCommand::CloseProject => {
                    self.status_message = Some("Project closed".into());
                    self.console.log(LogLevel::Info, "Project closed");
                }
            },
            Command::Edit(cmd) => match cmd {
                EditCommand::Undo | EditCommand::Redo => {
                    self.console
                        .log(LogLevel::Warning, "Undo/Redo stack not available yet");
                }
                EditCommand::Cut => self.console.log(LogLevel::Warning, "Cut not implemented"),
                EditCommand::Copy => self.console.log(LogLevel::Info, "Copied selection"),
                EditCommand::Paste => self.console.log(LogLevel::Warning, "Paste not implemented"),
                EditCommand::Delete => self.console.log(LogLevel::Info, "Delete selection"),
                EditCommand::SelectAll => {
                    self.console.log(LogLevel::Info, "Select All invoked");
                }
                EditCommand::Preferences => {
                    self.status_message = Some("Preferences not available".into());
                    self.console
                        .log(LogLevel::Warning, "Preferences dialog not implemented");
                }
            },
            Command::View(cmd) => match cmd {
                ViewCommand::ToggleMixer => {
                    self.mixer_hidden = !self.mixer_hidden;
                    let state = if self.mixer_hidden { "hidden" } else { "shown" };
                    self.console.log(LogLevel::Info, format!("Mixer {state}"));
                }
                ViewCommand::TogglePianoRoll => {
                    self.piano_roll_hidden = !self.piano_roll_hidden;
                    let state = if self.piano_roll_hidden {
                        "hidden"
                    } else {
                        "shown"
                    };
                    self.console
                        .log(LogLevel::Info, format!("Piano Roll {state}"));
                }
                ViewCommand::ToggleBrowser => {
                    self.browser_hidden = !self.browser_hidden;
                    self.browser.set_visible(!self.browser_hidden);
                    self.layout.set_browser_visible(!self.browser_hidden);
                    let state = if self.browser_hidden {
                        "hidden"
                    } else {
                        "shown"
                    };
                    self.console.log(LogLevel::Info, format!("Browser {state}"));
                }
                ViewCommand::ZoomIn => {
                    self.console.log(LogLevel::Info, "Zoom In");
                }
                ViewCommand::ZoomOut => {
                    self.console.log(LogLevel::Info, "Zoom Out");
                }
                ViewCommand::ToggleFullscreen => {
                    self.fullscreen = !self.fullscreen;
                    self.fullscreen_dirty = true;
                    self.console
                        .log(LogLevel::Info, "Fullscreen toggle requested");
                }
            },
            Command::Insert(cmd) => match cmd {
                InsertCommand::AudioTrack => {
                    self.console.log(LogLevel::Info, "Audio track inserted");
                }
                InsertCommand::MidiTrack => {
                    self.console.log(LogLevel::Info, "MIDI track inserted");
                }
                InsertCommand::ReturnBus => {
                    self.console.log(LogLevel::Info, "Return/Aux bus inserted");
                }
                InsertCommand::AddPluginOnSelectedTrack(category) => {
                    self.console.log(
                        LogLevel::Info,
                        format!("Plugin picker for {category} requested"),
                    );
                }
            },
            Command::Track(cmd) => match cmd {
                TrackCommand::ArmRecord => {
                    self.record_armed = !self.record_armed;
                    self.handle_transport_event(TransportEvent::Record(self.record_armed));
                }
                TrackCommand::Solo => {
                    self.console.log(LogLevel::Info, "Solo track");
                }
                TrackCommand::Mute => {
                    self.console.log(LogLevel::Info, "Mute track");
                }
                TrackCommand::FreezeCommit => {
                    self.console
                        .log(LogLevel::Warning, "Freeze/Commit not implemented");
                }
                TrackCommand::Rename => {
                    self.console
                        .log(LogLevel::Info, "Rename track dialog requested");
                }
                TrackCommand::Color => {
                    self.console
                        .log(LogLevel::Info, "Track color dialog requested");
                }
            },
            Command::Midi(cmd) => match cmd {
                MidiCommand::OpenInputDevicePicker => {
                    self.console
                        .log(LogLevel::Info, "Open MIDI input device picker");
                }
                MidiCommand::SelectInputDevice(device) => {
                    self.selected_midi_input = Some(device.clone());
                    self.console
                        .log(LogLevel::Info, format!("MIDI input set to {device}"));
                }
                MidiCommand::OpenChannelPicker => {
                    self.console.log(LogLevel::Info, "Open MIDI channel picker");
                }
                MidiCommand::SelectChannel(channel) => {
                    self.midi_channel = channel;
                    self.console
                        .log(LogLevel::Info, format!("MIDI channel set to {channel}"));
                }
                MidiCommand::Quantize => {
                    self.console.log(LogLevel::Info, "Quantize selection");
                }
                MidiCommand::Humanize => {
                    self.console
                        .log(LogLevel::Warning, "Humanize not implemented");
                }
                MidiCommand::MetronomeSettings => {
                    self.console
                        .log(LogLevel::Info, "Metronome settings dialog requested");
                }
            },
            Command::Transport(cmd) => match cmd {
                TransportCommand::TogglePlayPause => {
                    if matches!(
                        self.transport_state,
                        TransportState::Playing | TransportState::Recording
                    ) {
                        self.handle_transport_event(TransportEvent::Stop);
                    } else {
                        self.handle_transport_event(TransportEvent::Play);
                    }
                }
                TransportCommand::Stop => {
                    self.handle_transport_event(TransportEvent::Stop);
                }
                TransportCommand::RecordArm => {
                    self.record_armed = !self.record_armed;
                    self.handle_transport_event(TransportEvent::Record(self.record_armed));
                }
                TransportCommand::ToggleLoop => {
                    self.transport_loop_enabled = !self.transport_loop_enabled;
                    self.console.log(
                        LogLevel::Info,
                        format!(
                            "Loop {}",
                            if self.transport_loop_enabled {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        ),
                    );
                }
                TransportCommand::LoopToSelection => {
                    self.console.log(LogLevel::Info, "Loop to selection");
                }
                TransportCommand::GoToStart => {
                    self.transport.sample_pos.store(0, AtomicOrdering::Relaxed);
                    self.console.log(LogLevel::Info, "Go to start");
                }
                TransportCommand::TapTempo => {
                    self.console.log(LogLevel::Info, "Tap tempo");
                }
            },
            Command::Options(cmd) => match cmd {
                OptionsCommand::AudioDeviceDialog => {
                    let config = self.engine_runner.config().clone();
                    let runtime = self.engine_runner.runtime_options().clone();
                    self.audio_settings.open(&config, &runtime);
                }
                OptionsCommand::ProjectSettings => {
                    self.console
                        .log(LogLevel::Info, "Project settings dialog requested");
                }
                OptionsCommand::Theme(mode) => {
                    self.console
                        .log(LogLevel::Info, format!("Theme set to {:?}", mode));
                }
                OptionsCommand::CpuMeter => {
                    self.console.log(LogLevel::Info, "CPU meter toggled");
                }
            },
            Command::Help(cmd) => match cmd {
                HelpCommand::About => {
                    self.status_message = Some("Harmoniq Studio prototype".into());
                    self.console.log(LogLevel::Info, "About dialog requested");
                }
                HelpCommand::OpenLogsFolder => {
                    self.console.log(LogLevel::Info, "Open logs folder");
                }
                HelpCommand::UserManual => {
                    self.console.log(LogLevel::Info, "Open user manual");
                }
            },
        }
    }
}

struct WorkspaceTabViewer<'a> {
    palette: &'a HarmoniqPalette,
    event_bus: &'a EventBus,
    browser: &'a mut BrowserPane,
    channel_rack: &'a mut ChannelRackPane,
    piano_roll: &'a mut PianoRollPane,
    mixer: &'a mut MixerPane,
    playlist: &'a mut PlaylistPane,
    inspector: &'a mut InspectorPane,
    console: &'a mut ConsolePane,
    input_focus: &'a mut InputFocus,
    browser_hidden: bool,
    mixer_hidden: bool,
    piano_roll_hidden: bool,
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
            WorkspacePane::Browser => {
                if self.browser_hidden {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Browser hidden (View → Toggle Browser)")
                                .color(self.palette.text_muted),
                        );
                    });
                } else {
                    self.browser
                        .ui(ui, self.palette, self.event_bus, self.input_focus);
                }
            }
            WorkspacePane::Arrange => self.playlist.ui(
                ui,
                self.palette,
                self.event_bus,
                self.input_focus,
                self.transport_state,
                self.transport_clock,
            ),
            WorkspacePane::Mixer => {
                if self.mixer_hidden {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Mixer hidden (View → Toggle Mixer)")
                                .color(self.palette.text_muted),
                        );
                    });
                } else {
                    self.mixer
                        .ui(ui, self.palette, self.event_bus, self.input_focus);
                }
            }
            WorkspacePane::PianoRoll => {
                if self.piano_roll_hidden {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Piano Roll hidden (View → Toggle Piano Roll)")
                                .color(self.palette.text_muted),
                        );
                    });
                } else {
                    self.piano_roll
                        .ui(ui, self.palette, self.event_bus, self.input_focus);
                }
            }
            WorkspacePane::Inspector => {
                let commands = self.inspector.ui(
                    ui,
                    self.palette,
                    self.input_focus,
                    self.event_bus,
                    self.channel_rack,
                );
                for command in commands {
                    self.playlist.apply_inspector_command(command);
                }
                self.inspector
                    .sync_selection(self.playlist.current_selection());
            }
            WorkspacePane::Console => {
                self.console.ui(ui, self.palette, self.input_focus);
            }
        }
    }
}

impl App for HarmoniqStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(16));
        self.shortcuts.handle_input(ctx, &self.command_sender);
        self.process_qwerty_keyboard(ctx);
        for command in self.command_dispatcher.drain_pending() {
            self.handle_command(command);
        }
        if self.fullscreen_dirty {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
            self.fullscreen_dirty = false;
        }
        self.process_events();
        self.update_engine_context();
        self.input_focus.maybe_release_on_escape(ctx);
        self.inspector
            .sync_selection(self.playlist.current_selection());

        let palette = self.theme.palette().clone();

        egui::TopBottomPanel::top("menu_bar")
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .outer_margin(Margin::symmetric(8.0, 4.0)),
            )
            .show(ctx, |ui| {
                let snapshot = MenuBarSnapshot {
                    mixer_visible: !self.mixer_hidden,
                    piano_roll_visible: !self.piano_roll_hidden,
                    browser_visible: !self.browser_hidden,
                    fullscreen: self.fullscreen,
                    can_undo: false,
                    can_redo: false,
                    transport_playing: matches!(
                        self.transport_state,
                        TransportState::Playing | TransportState::Recording
                    ),
                    transport_record_armed: self.record_armed,
                    transport_loop_enabled: self.transport_loop_enabled,
                    recent_projects: self.recent_projects.entries(),
                    midi_inputs: &self.midi_inputs,
                    selected_midi_input: self.selected_midi_input.as_deref(),
                    midi_channel: self.midi_channel,
                };
                self.menu.render(
                    ui,
                    &palette,
                    &self.shortcuts,
                    &self.command_sender,
                    &snapshot,
                );
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
                    browser: &mut self.browser,
                    channel_rack: &mut self.channel_rack,
                    piano_roll: &mut self.piano_roll,
                    mixer: &mut self.mixer,
                    playlist: &mut self.playlist,
                    inspector: &mut self.inspector,
                    console: &mut self.console,
                    input_focus: &mut self.input_focus,
                    browser_hidden: self.browser_hidden,
                    mixer_hidden: self.mixer_hidden,
                    piano_roll_hidden: self.piano_roll_hidden,
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

        if let Some(action) = self.audio_settings.ui(
            ctx,
            &palette,
            active_audio_summary.as_ref(),
            last_runtime_error.as_deref(),
        ) {
            match action {
                AudioSettingsAction::Apply { config, runtime } => {
                    let result = self
                        .engine_runner
                        .reconfigure(config.clone(), runtime.clone());
                    self.audio_settings
                        .on_apply_result(result, &config, &runtime);
                }
                AudioSettingsAction::PlayTestSound => {
                    self.play_sound_test();
                }
            }
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
