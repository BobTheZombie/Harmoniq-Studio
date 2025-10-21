use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use eframe::egui::Margin;
use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Rect, Rgba, RichText, Rounding, Sense, Stroke,
    TextStyle, TextureOptions, Vec2,
};
use eframe::{App, CreationContext, NativeOptions};
use egui_extras::{image::load_svg_bytes, install_image_loaders};
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine,
    TransportState,
};
use harmoniq_plugins::{GainPlugin, NoisePlugin, SineSynth, WestCoastLead};
use hound::{SampleFormat, WavSpec, WavWriter};
use mp3lame_encoder::{
    self, Builder as Mp3Builder, FlushNoGap, InterleavedPcm, MonoPcm, Quality as Mp3Quality,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;

mod audio;
mod midi;
mod typing_keyboard;

use audio::{
    available_backends, available_output_devices, describe_layout, AudioBackend,
    AudioRuntimeOptions, OutputDeviceInfo, RealtimeAudio,
};
use midi::list_midi_inputs;
use typing_keyboard::TypingKeyboard;
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
struct HarmoniqPalette {
    background: Color32,
    panel: Color32,
    panel_alt: Color32,
    toolbar: Color32,
    toolbar_highlight: Color32,
    toolbar_outline: Color32,
    text_primary: Color32,
    text_muted: Color32,
    accent: Color32,
    accent_alt: Color32,
    accent_soft: Color32,
    success: Color32,
    warning: Color32,
    track_header: Color32,
    track_header_selected: Color32,
    track_lane_overlay: Color32,
    track_button_bg: Color32,
    track_button_border: Color32,
    automation_header: Color32,
    automation_header_muted: Color32,
    automation_lane_bg: Color32,
    automation_lane_hidden_bg: Color32,
    automation_point_border: Color32,
    timeline_bg: Color32,
    timeline_header: Color32,
    timeline_border: Color32,
    timeline_grid_primary: Color32,
    timeline_grid_secondary: Color32,
    ruler_text: Color32,
    clip_text_primary: Color32,
    clip_text_secondary: Color32,
    clip_border_default: Color32,
    clip_border_active: Color32,
    clip_border_playing: Color32,
    clip_shadow: Color32,
    piano_background: Color32,
    piano_grid_major: Color32,
    piano_grid_minor: Color32,
    piano_white: Color32,
    piano_black: Color32,
    meter_background: Color32,
    meter_border: Color32,
    meter_left: Color32,
    meter_right: Color32,
    meter_rms: Color32,
    knob_base: Color32,
    knob_ring: Color32,
    knob_indicator: Color32,
    knob_label: Color32,
    mixer_strip_bg: Color32,
    mixer_strip_selected: Color32,
    mixer_strip_solo: Color32,
    mixer_strip_muted: Color32,
    mixer_strip_border: Color32,
}

impl HarmoniqPalette {
    fn new() -> Self {
        Self {
            background: Color32::from_rgb(16, 18, 32),
            panel: Color32::from_rgb(28, 30, 48),
            panel_alt: Color32::from_rgb(36, 38, 60),
            toolbar: Color32::from_rgb(32, 36, 62),
            toolbar_highlight: Color32::from_rgb(48, 54, 88),
            toolbar_outline: Color32::from_rgb(74, 78, 118),
            text_primary: Color32::from_rgb(240, 242, 255),
            text_muted: Color32::from_rgb(176, 182, 214),
            accent: Color32::from_rgb(255, 152, 92),
            accent_alt: Color32::from_rgb(134, 190, 255),
            accent_soft: Color32::from_rgb(255, 108, 208),
            success: Color32::from_rgb(112, 220, 176),
            warning: Color32::from_rgb(255, 112, 132),
            track_header: Color32::from_rgb(52, 56, 86),
            track_header_selected: Color32::from_rgb(76, 82, 124),
            track_lane_overlay: Color32::from_rgba_unmultiplied(132, 180, 255, 40),
            track_button_bg: Color32::from_rgb(64, 66, 92),
            track_button_border: Color32::from_rgb(30, 32, 48),
            automation_header: Color32::from_rgb(60, 64, 100),
            automation_header_muted: Color32::from_rgb(46, 50, 78),
            automation_lane_bg: Color32::from_rgb(34, 38, 64),
            automation_lane_hidden_bg: Color32::from_rgb(28, 30, 52),
            automation_point_border: Color32::from_rgb(28, 28, 40),
            timeline_bg: Color32::from_rgb(24, 26, 46),
            timeline_header: Color32::from_rgb(42, 46, 74),
            timeline_border: Color32::from_rgb(74, 78, 118),
            timeline_grid_primary: Color32::from_rgb(94, 104, 150),
            timeline_grid_secondary: Color32::from_rgb(58, 62, 92),
            ruler_text: Color32::from_rgb(204, 208, 238),
            clip_text_primary: Color32::from_rgb(246, 248, 255),
            clip_text_secondary: Color32::from_rgb(206, 210, 238),
            clip_border_default: Color32::from_rgb(36, 38, 54),
            clip_border_active: Color32::from_rgb(232, 232, 248),
            clip_border_playing: Color32::from_rgb(255, 214, 128),
            clip_shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 90),
            piano_background: Color32::from_rgb(18, 20, 36),
            piano_grid_major: Color32::from_rgb(84, 90, 130),
            piano_grid_minor: Color32::from_rgb(48, 52, 78),
            piano_white: Color32::from_rgb(240, 244, 255),
            piano_black: Color32::from_rgb(60, 62, 92),
            meter_background: Color32::from_rgb(34, 36, 60),
            meter_border: Color32::from_rgb(84, 88, 128),
            meter_left: Color32::from_rgb(92, 218, 255),
            meter_right: Color32::from_rgb(255, 138, 184),
            meter_rms: Color32::from_rgb(255, 226, 132),
            knob_base: Color32::from_rgb(48, 50, 82),
            knob_ring: Color32::from_rgb(255, 162, 118),
            knob_indicator: Color32::from_rgb(140, 234, 255),
            knob_label: Color32::from_rgb(214, 218, 250),
            mixer_strip_bg: Color32::from_rgb(44, 46, 74),
            mixer_strip_selected: Color32::from_rgb(66, 70, 106),
            mixer_strip_solo: Color32::from_rgb(60, 98, 88),
            mixer_strip_muted: Color32::from_rgb(104, 62, 78),
            mixer_strip_border: Color32::from_rgb(82, 86, 122),
        }
    }
}

struct HarmoniqTheme {
    palette: HarmoniqPalette,
}

impl HarmoniqTheme {
    fn init(ctx: &egui::Context) -> Self {
        let palette = HarmoniqPalette::new();
        let mut style = (*ctx.style()).clone();
        let mut visuals = style.visuals.clone();
        visuals.dark_mode = true;
        visuals.override_text_color = Some(palette.text_primary);
        visuals.panel_fill = palette.background;
        visuals.window_fill = palette.panel;
        visuals.window_stroke = Stroke::new(1.0, palette.toolbar_outline);
        visuals.widgets.noninteractive.bg_fill = palette.panel;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_muted);
        visuals.widgets.inactive.bg_fill = palette.panel_alt;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text_primary);
        visuals.widgets.hovered.bg_fill = palette.accent_alt.gamma_multiply(0.7);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text_primary);
        visuals.widgets.active.bg_fill = palette.accent.gamma_multiply(0.8);
        visuals.widgets.active.fg_stroke = Stroke::new(1.2, palette.text_primary);
        visuals.widgets.open.bg_fill = palette.toolbar_highlight;
        visuals.selection.bg_fill = palette.accent_soft.gamma_multiply(0.85);
        visuals.selection.stroke = Stroke::new(1.0, palette.accent_alt);
        visuals.hyperlink_color = palette.accent_alt;
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(18.0, 10.0);
        style.spacing.window_margin = Margin::same(12.0);
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(26.0)),
            (TextStyle::Body, FontId::proportional(17.0)),
            (TextStyle::Button, FontId::proportional(16.0)),
            (TextStyle::Small, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(15.0)),
        ]
        .into();
        ctx.set_style(style);
        Self { palette }
    }

    fn palette(&self) -> &HarmoniqPalette {
        &self.palette
    }
}

struct AppIcons {
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
            Ok(ctx.load_texture(name, image, TextureOptions::LINEAR))
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
    if !(1..=2).contains(&channels) {
        anyhow::bail!("MP3 export only supports mono or stereo audio");
    }

    let mut builder =
        Mp3Builder::new().ok_or_else(|| anyhow!("failed to initialise MP3 encoder"))?;
    builder
        .set_sample_rate(sample_rate.round() as u32)
        .map_err(|err| anyhow!("failed to configure MP3 sample rate: {err}"))?;
    builder
        .set_num_channels(channels as u8)
        .map_err(|err| anyhow!("failed to configure MP3 channels: {err}"))?;
    builder
        .set_quality(Mp3Quality::Best)
        .map_err(|err| anyhow!("failed to configure MP3 quality: {err}"))?;
    let mut encoder = builder
        .build()
        .map_err(|err| anyhow!("failed to start MP3 encoder: {err}"))?;

    let samples_per_channel = samples.len() / channels.max(1);
    let mut output = Vec::new();
    output.reserve(mp3lame_encoder::max_required_buffer_size(
        samples_per_channel,
    ));

    let encoded = match channels {
        1 => encoder
            .encode(MonoPcm(samples), output.spare_capacity_mut())
            .map_err(|err| anyhow!("failed to encode MP3 audio: {err:?}"))?,
        2 => encoder
            .encode(InterleavedPcm(samples), output.spare_capacity_mut())
            .map_err(|err| anyhow!("failed to encode MP3 audio: {err:?}"))?,
        _ => unreachable!(),
    };
    unsafe {
        output.set_len(output.len() + encoded);
    }

    let flushed = encoder
        .flush::<FlushNoGap>(output.spare_capacity_mut())
        .map_err(|err| anyhow!("failed to finalise MP3 audio: {err:?}"))?;
    unsafe {
        output.set_len(output.len() + flushed);
    }

    fs::write(path, &output).context("failed to write MP3 file")?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .ok();

    let args = Cli::parse();

    if args.list_audio_backends {
        let hosts = available_backends();
        if hosts.is_empty() {
            println!("No realtime audio hosts reported by the system.");
        } else {
            println!("Available realtime audio backends:");
            for (backend, host_name) in hosts {
                println!("  - {backend} ({host_name})");
            }
        }
        return Ok(());
    }

    if args.list_midi_inputs {
        let ports = list_midi_inputs().context("failed to list MIDI inputs")?;
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
                    running_clone.store(false, Ordering::SeqCst);
                })?;

                while running.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "failed to start realtime audio output; running offline instead"
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

    fn refresh_runtime(&mut self) -> anyhow::Result<()> {
        if let Some(mut loop_state) = self.offline_loop.take() {
            loop_state.stop();
        }
        self.realtime = None;
        self.last_runtime_error = None;

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

    fn reconfigure_audio(&mut self, runtime: AudioRuntimeOptions) -> anyhow::Result<()> {
        let previous = self.runtime.clone();
        self.runtime = runtime;
        if let Err(err) = self.refresh_runtime() {
            self.runtime = previous;
            if let Err(revert) = self.refresh_runtime() {
                error!(
                    ?revert,
                    "failed to restore previous audio runtime after error"
                );
            }
            return Err(err);
        }
        Ok(())
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

impl Drop for EngineRunner {
    fn drop(&mut self) {
        if let Some(mut loop_state) = self.offline_loop.take() {
            loop_state.stop();
        }
    }
}

struct OfflineLoop {
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OfflineLoop {
    fn start(engine: Arc<Mutex<HarmoniqEngine>>, config: BufferConfig) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let engine_clone = Arc::clone(&engine);
        let handle = std::thread::spawn(move || {
            let mut buffer = AudioBuffer::from_config(config.clone());
            while thread_running.load(Ordering::SeqCst) {
                {
                    let mut engine = engine_clone.lock();
                    if let Err(err) = engine.process_block(&mut buffer) {
                        error!(?err, "engine processing failed");
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        Self {
            running,
            thread: Some(handle),
        }
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for OfflineLoop {
    fn drop(&mut self) {
        self.stop();
    }
}

struct AudioSettingsState {
    selected_backend: AudioBackend,
    selected_device: Option<String>,
    available_devices: Vec<OutputDeviceInfo>,
    active_backend: Option<AudioBackend>,
    active_device_name: Option<String>,
    active_device_id: Option<String>,
    active_host_label: Option<String>,
    error: Option<String>,
}

impl AudioSettingsState {
    fn initialise(runtime: &AudioRuntimeOptions, realtime: Option<&RealtimeAudio>) -> Self {
        let mut state = Self {
            selected_backend: runtime.backend,
            selected_device: runtime.output_device.clone(),
            available_devices: Vec::new(),
            active_backend: None,
            active_device_name: None,
            active_device_id: None,
            active_host_label: None,
            error: None,
        };
        state.refresh_devices();
        state.update_active(realtime);
        state
    }

    fn refresh_devices(&mut self) {
        match available_output_devices(self.selected_backend) {
            Ok(devices) => {
                self.available_devices = devices;
                if let Some(selected) = &self.selected_device {
                    if !self
                        .available_devices
                        .iter()
                        .any(|info| &info.id == selected)
                    {
                        self.selected_device = None;
                    }
                }
                self.error = None;
            }
            Err(err) => {
                self.available_devices.clear();
                self.error = Some(err.to_string());
                self.selected_device = None;
            }
        }
    }

    fn set_selected_backend(&mut self, backend: AudioBackend) {
        if self.selected_backend != backend {
            self.selected_backend = backend;
            self.selected_device = None;
            self.refresh_devices();
        }
    }

    fn set_selected_device(&mut self, device: Option<String>) {
        self.selected_device = device;
    }

    fn update_active(&mut self, realtime: Option<&RealtimeAudio>) {
        self.active_backend = realtime.map(|rt| rt.backend());
        self.active_device_name = realtime.map(|rt| rt.device_name().to_string());
        self.active_device_id = realtime.and_then(|rt| rt.device_id().map(|id| id.to_string()));
        self.active_host_label =
            realtime.and_then(|rt| rt.host_label().map(|label| label.to_string()));
    }

    fn sync_with_runtime(
        &mut self,
        runtime: &AudioRuntimeOptions,
        realtime: Option<&RealtimeAudio>,
    ) {
        self.selected_backend = runtime.backend;
        self.selected_device = runtime.output_device.clone();
        self.refresh_devices();
        self.update_active(realtime);
    }

    fn selected_backend_label(&self, options: &[(AudioBackend, String)]) -> String {
        options
            .iter()
            .find(|(backend, _)| *backend == self.selected_backend)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| self.selected_backend.to_string())
    }

    fn selected_device_label(&self) -> String {
        self.selected_device
            .as_ref()
            .and_then(|selected| {
                self.available_devices
                    .iter()
                    .find(|info| &info.id == selected)
                    .map(|info| info.label.clone())
            })
            .unwrap_or_else(|| "System Default".to_string())
    }
}

struct WestCoastEditorState {
    plugin: WestCoastLead,
    show: bool,
    last_error: Option<String>,
}

impl WestCoastEditorState {
    fn new(sample_rate: f32) -> Self {
        let mut plugin = WestCoastLead::default();
        plugin.set_sample_rate(sample_rate);
        Self {
            plugin,
            show: true,
            last_error: None,
        }
    }

    fn open(&mut self) {
        self.show = true;
    }

    fn knob(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        id: &str,
        min: f32,
        max: f32,
        fallback_default: f32,
        label: &str,
        description: &str,
    ) {
        let default = self
            .plugin
            .parameter_default(id)
            .unwrap_or(fallback_default);
        let mut value = self.plugin.parameter_value(id).unwrap_or(default);
        let response = ui
            .add(Knob::new(&mut value, min, max, default, label, palette))
            .on_hover_text(format!("{description}\nCurrent: {value:.3}"));
        if response.changed() {
            match self.plugin.set_parameter_from_ui(id, value) {
                Ok(()) => self.last_error = None,
                Err(err) => self.last_error = Some(err),
            }
        }
    }

    fn draw(&mut self, ctx: &egui::Context, palette: &HarmoniqPalette) {
        if !self.show {
            return;
        }
        let mut open = self.show;
        egui::Window::new("West Coast Lead")
            .open(&mut open)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| self.draw_contents(ui, palette));
        self.show = open;
    }

    fn draw_contents(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette) {
        ui.heading(RichText::new("West Coast Lead").color(palette.text_primary));
        ui.label(
            RichText::new(
                "Wavefolded sine lead with tone, low-pass gate, and modulation controls.",
            )
            .color(palette.text_muted),
        );
        ui.add_space(12.0);

        ui.label(
            RichText::new("Performance")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "westcoast.level",
                0.0,
                1.2,
                0.9,
                "Level",
                "Output level after the low-pass gate",
            );
            self.knob(
                ui,
                palette,
                "westcoast.glide",
                0.0,
                0.4,
                0.12,
                "Glide",
                "Portamento time between incoming notes",
            );
            self.knob(
                ui,
                palette,
                "westcoast.sub_mix",
                0.0,
                0.8,
                0.25,
                "Sub Mix",
                "Blend in the supportive sub-octave sine",
            );
            self.knob(
                ui,
                palette,
                "westcoast.noise_mix",
                0.0,
                0.4,
                0.08,
                "Noise",
                "Add airy vinyl hiss to the tone",
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label(
            RichText::new("Amplitude Envelope")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "westcoast.attack",
                0.001,
                0.3,
                0.02,
                "Attack",
                "Rise time of the low-pass gate",
            );
            self.knob(
                ui,
                palette,
                "westcoast.decay",
                0.05,
                1.0,
                0.18,
                "Decay",
                "Time to fall to sustain",
            );
            self.knob(
                ui,
                palette,
                "westcoast.sustain",
                0.3,
                1.0,
                0.8,
                "Sustain",
                "Steady level while the note is held",
            );
            self.knob(
                ui,
                palette,
                "westcoast.release",
                0.05,
                1.5,
                0.26,
                "Release",
                "Gate fall time after key release",
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label(
            RichText::new("Pitch & Vibrato")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "westcoast.vibrato_rate",
                0.1,
                8.0,
                4.8,
                "Vibrato Hz",
                "Sine vibrato rate (mod wheel increases depth)",
            );
            self.knob(
                ui,
                palette,
                "westcoast.vibrato_depth",
                0.0,
                0.02,
                0.008,
                "Vibrato Amt",
                "Vibrato depth before modulation wheel",
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label(
            RichText::new("Timbre & Tone")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "westcoast.timbre",
                0.0,
                1.0,
                0.45,
                "Timbre",
                "Crossfade between pure sine and wavefolded harmonics",
            );
            self.knob(
                ui,
                palette,
                "westcoast.tone",
                0.0,
                1.0,
                0.6,
                "Tone",
                "Base cutoff of the low-pass gate",
            );
            self.knob(
                ui,
                palette,
                "westcoast.tone_mod",
                0.0,
                1.0,
                0.5,
                "Tone Mod",
                "Envelope amount opening the gate brightness",
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label(
            RichText::new("West Coast Modulation")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "westcoast.mod_rate",
                0.05,
                12.0,
                2.5,
                "Fold Rate",
                "Timbre LFO rate for animated wavefolds",
            );
            self.knob(
                ui,
                palette,
                "westcoast.mod_depth",
                0.0,
                1.0,
                0.45,
                "Fold Amt",
                "Depth of the timbre LFO",
            );
        });

        ui.add_space(12.0);
        ui.label(
            RichText::new("Tip: double-click any knob to restore its default value.")
                .color(palette.text_muted)
                .small(),
        );

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Reset to defaults").clicked() {
                self.plugin.reset_to_defaults();
                self.last_error = None;
            }
            if let Some(error) = &self.last_error {
                ui.add_space(12.0);
                ui.label(RichText::new(error).color(palette.warning));
            }
        });
    }
}

struct HarmoniqStudioApp {
    theme: HarmoniqTheme,
    icons: AppIcons,
    engine_runner: EngineRunner,
    command_queue: harmoniq_engine::EngineCommandQueue,
    typing_keyboard: TypingKeyboard,
    engine_config: BufferConfig,
    graph_config: DemoGraphConfig,
    tracks: Vec<Track>,
    master_track: MasterChannel,
    selected_track: Option<usize>,
    selected_clip: Option<(usize, usize)>,
    tempo: f32,
    transport_state: TransportState,
    next_track_index: usize,
    next_clip_index: usize,
    next_color_index: usize,
    playlist: PlaylistViewState,
    piano_roll: PianoRollState,
    last_error: Option<String>,
    status_message: Option<String>,
    audio_settings: AudioSettingsState,
    westcoast_editor: WestCoastEditorState,
    project_path: String,
    bounce_path: String,
    bounce_length_beats: f32,
}

impl HarmoniqStudioApp {
    fn new(
        config: BufferConfig,
        initial_tempo: f32,
        runtime: AudioRuntimeOptions,
        cc: &CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        let theme = HarmoniqTheme::init(&cc.egui_ctx);
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

        let audio_settings = AudioSettingsState::initialise(
            engine_runner.runtime_options(),
            engine_runner.realtime(),
        );

        let startup_status = engine_runner.last_runtime_error().map(|err| {
            format!("Realtime audio failed to start: {err}. Engine is running offline.")
        });

        let tracks: Vec<Track> = (0..48).map(|index| Track::with_index(index + 1)).collect();
        let track_count = tracks.len();
        let master_track = MasterChannel::default();

        let mut app = Self {
            theme,
            icons,
            engine_runner,
            command_queue,
            typing_keyboard: TypingKeyboard::default(),
            engine_config: config.clone(),
            graph_config,
            tracks,
            master_track,
            selected_track: Some(0),
            selected_clip: Some((0, 0)),
            tempo: initial_tempo,
            transport_state: TransportState::Stopped,
            next_track_index: track_count,
            next_clip_index: 1,
            next_color_index: 0,
            playlist: PlaylistViewState::default(),
            piano_roll: PianoRollState::default(),
            last_error: None,
            status_message: startup_status,
            audio_settings,
            westcoast_editor: WestCoastEditorState::new(config.sample_rate),
            project_path: "project.hst".into(),
            bounce_path: "bounce.wav".into(),
            bounce_length_beats: 16.0,
        };

        app.initialise_demo_clips();
        Ok(app)
    }

    fn initialise_demo_clips(&mut self) {
        if let Some(track) = self.tracks.get_mut(0) {
            track.name = "Lead".into();
        }
        if let Some(track) = self.tracks.get_mut(1) {
            track.name = "Drums".into();
        }
        if let Some(track) = self.tracks.get_mut(2) {
            track.name = "Bass".into();
        }

        let lead_intro_color = self.next_color();
        let lead_hook_color = self.next_color();
        let lead_automation_color = self.next_color();
        if let Some(track) = self.tracks.get_mut(0) {
            track.add_clip(Clip::new(
                "Lead Intro",
                0.0,
                8.0,
                lead_intro_color,
                vec![Note::new(0.0, 1.0, 72), Note::new(1.0, 1.0, 76)],
            ));
            track.add_clip(Clip::new(
                "Lead Hook",
                8.0,
                8.0,
                lead_hook_color,
                vec![Note::new(0.0, 0.5, 79), Note::new(1.0, 0.5, 81)],
            ));
            track.add_automation_lane(AutomationLane::new(
                "Lead Filter",
                lead_automation_color,
                vec![
                    AutomationPoint::new(0.0, 0.3),
                    AutomationPoint::new(4.0, 0.8),
                    AutomationPoint::new(8.0, 0.5),
                ],
            ));
        }

        let drum_color = self.next_color();
        let drum_automation_color = self.next_color();
        if let Some(track) = self.tracks.get_mut(1) {
            track.add_clip(Clip::new(
                "Drum Loop",
                0.0,
                16.0,
                drum_color,
                vec![Note::new(0.0, 0.5, 36), Note::new(0.5, 0.5, 38)],
            ));
            track.add_automation_lane(AutomationLane::new(
                "Drum Reverb",
                drum_automation_color,
                vec![
                    AutomationPoint::new(0.0, 0.2),
                    AutomationPoint::new(8.0, 0.6),
                ],
            ));
        }

        let bass_color = self.next_color();
        if let Some(track) = self.tracks.get_mut(2) {
            track.add_clip(Clip::new(
                "Bass Line",
                0.0,
                16.0,
                bass_color,
                vec![Note::new(0.0, 1.0, 48), Note::new(2.0, 1.0, 43)],
            ));
        }
    }

    fn next_color(&mut self) -> Color32 {
        const COLORS: [Color32; 8] = [
            Color32::from_rgb(0x7f, 0xc8, 0xff),
            Color32::from_rgb(0xff, 0xa7, 0x26),
            Color32::from_rgb(0xff, 0x7f, 0xd5),
            Color32::from_rgb(0x5e, 0xff, 0xa1),
            Color32::from_rgb(0xff, 0xd6, 0x4f),
            Color32::from_rgb(0xc0, 0x89, 0xff),
            Color32::from_rgb(0xff, 0x9d, 0x9d),
            Color32::from_rgb(0x4f, 0xff, 0xe8),
        ];

        let color = COLORS[self.next_color_index % COLORS.len()];
        self.next_color_index += 1;
        color
    }

    fn palette(&self) -> &HarmoniqPalette {
        self.theme.palette()
    }

    fn gradient_icon_button(
        &self,
        ui: &mut egui::Ui,
        icon: &egui::TextureHandle,
        label: &str,
        gradient: (Color32, Color32),
        active: bool,
        size: Vec2,
    ) -> egui::Response {
        let desired_size = Vec2::new(size.x.max(96.0), size.y.max(36.0));
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        let palette = self.palette().clone();
        let mut start = gradient.0;
        let mut end = gradient.1;

        if active {
            start = start.gamma_multiply(1.1);
            end = end.gamma_multiply(1.1);
        }
        if response.hovered() {
            start = start.gamma_multiply(1.05);
            end = end.gamma_multiply(1.05);
        }
        if response.is_pointer_button_down_on() {
            start = start.gamma_multiply(0.95);
            end = end.gamma_multiply(0.95);
        }

        let rounding = Rounding::same((desired_size.y * 0.48).clamp(8.0, 24.0));
        let painter = ui.painter();
        let start_rgba = Rgba::from(start);
        let end_rgba = Rgba::from(end);
        const STEPS: usize = 24;
        for i in 0..STEPS {
            let t0 = i as f32 / STEPS as f32;
            let t1 = (i + 1) as f32 / STEPS as f32;
            let left = egui::lerp(rect.left()..=rect.right(), t0);
            let right = if i == STEPS - 1 {
                rect.right()
            } else {
                egui::lerp(rect.left()..=rect.right(), t1)
            };
            let segment_rect = Rect::from_min_max(
                egui::pos2(left, rect.top()),
                egui::pos2(right, rect.bottom()),
            );
            let t_mid = (t0 + t1) * 0.5;
            let color = Color32::from(start_rgba * (1.0 - t_mid) + end_rgba * t_mid);
            let segment_rounding = if i == 0 {
                Rounding {
                    nw: rounding.nw,
                    ne: 0.0,
                    sw: rounding.sw,
                    se: 0.0,
                }
            } else if i == STEPS - 1 {
                Rounding {
                    nw: 0.0,
                    ne: rounding.ne,
                    sw: 0.0,
                    se: rounding.se,
                }
            } else {
                Rounding::ZERO
            };
            painter.rect_filled(segment_rect, segment_rounding, color);
        }
        painter.rect_stroke(rect, rounding, Stroke::new(1.0, palette.toolbar_outline));

        let content_rect = rect.shrink2(egui::vec2(16.0, 10.0));
        let icon_side = (content_rect.height().min(28.0)).max(18.0);
        let icon_rect = Rect::from_min_size(
            egui::pos2(
                content_rect.left(),
                content_rect.center().y - icon_side * 0.5,
            ),
            Vec2::splat(icon_side),
        );
        ui.painter().image(
            icon.id(),
            icon_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        let text_pos = egui::pos2(icon_rect.right() + 10.0, content_rect.center().y);
        ui.painter().text(
            text_pos,
            Align2::LEFT_CENTER,
            label,
            FontId::proportional((desired_size.y * 0.42).clamp(14.0, 18.0)),
            palette.text_primary,
        );

        response
    }

    fn section_label(&self, ui: &mut egui::Ui, icon: &egui::TextureHandle, label: &str) {
        let palette = self.palette().clone();
        ui.horizontal(|ui| {
            let tint = palette.accent_alt;
            ui.add(
                egui::Image::from_texture(icon)
                    .fit_to_exact_size(Vec2::splat(18.0))
                    .tint(tint),
            );
            ui.label(
                RichText::new(label)
                    .color(palette.text_muted)
                    .strong()
                    .small(),
            );
        });
    }

    fn tinted_frame<F>(palette: &HarmoniqPalette, ui: &mut egui::Ui, fill: Color32, add_contents: F)
    where
        F: FnOnce(&mut egui::Ui),
    {
        egui::Frame::none()
            .fill(fill)
            .stroke(Stroke::new(1.0, palette.toolbar_outline))
            .rounding(Rounding::same(12.0))
            .inner_margin(Margin::symmetric(12.0, 8.0))
            .show(ui, add_contents);
    }

    fn send_command(&mut self, command: EngineCommand) {
        if let Err(command) = self.command_queue.try_send(command) {
            self.last_error = Some(format!("Command queue full: {command:?}"));
            self.status_message = None;
        } else {
            self.last_error = None;
        }
    }

    fn toggle_transport(&mut self) {
        self.transport_state = match self.transport_state {
            TransportState::Playing => TransportState::Stopped,
            _ => TransportState::Playing,
        };
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.status_message = Some(match self.transport_state {
            TransportState::Playing => "Transport playing".to_string(),
            TransportState::Stopped => "Transport paused".to_string(),
            TransportState::Recording => "Transport recording".to_string(),
        });
        if self.transport_state == TransportState::Stopped {
            self.stop_all_clips();
        }
    }

    fn stop_transport(&mut self) {
        self.transport_state = TransportState::Stopped;
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.stop_all_clips();
        self.status_message = Some("Transport stopped".into());
    }

    fn add_track(&mut self) {
        self.next_track_index += 1;
        self.tracks.push(Track::with_index(self.next_track_index));
    }

    fn stop_all_clips(&mut self) {
        for track in &mut self.tracks {
            track.stop_all_clips();
        }
    }

    fn any_clip_playing(&self) -> bool {
        self.tracks.iter().any(|track| track.has_playing_clips())
    }

    fn launch_clip(&mut self, track_idx: usize, clip_idx: usize) {
        let (track_name, clip_name) = match self.tracks.get(track_idx).and_then(|track| {
            track
                .clips
                .get(clip_idx)
                .map(|clip| (track.name.clone(), clip.name.clone()))
        }) {
            Some(names) => names,
            None => return,
        };

        if let Some(track) = self.tracks.get_mut(track_idx) {
            for (idx, clip) in track.clips.iter_mut().enumerate() {
                clip.launch_state = if idx == clip_idx {
                    ClipLaunchState::Playing
                } else {
                    ClipLaunchState::Stopped
                };
            }
        }

        self.transport_state = TransportState::Playing;
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.status_message = Some(format!("Launched '{clip_name}' on {track_name}"));
    }

    fn stop_clip(&mut self, track_idx: usize, clip_idx: usize) {
        let clip_name = self
            .tracks
            .get(track_idx)
            .and_then(|track| track.clips.get(clip_idx))
            .map(|clip| clip.name.clone());

        if let Some(track) = self.tracks.get_mut(track_idx) {
            if let Some(clip) = track.clips.get_mut(clip_idx) {
                clip.launch_state = ClipLaunchState::Stopped;
            }
        }

        if self.any_clip_playing() {
            if let Some(name) = clip_name {
                self.status_message = Some(format!("Stopped '{name}'"));
            }
        } else {
            self.stop_transport();
        }
    }

    fn bounce_project(&mut self) {
        match offline_bounce_to_file(
            &self.bounce_path,
            self.engine_config.clone(),
            &self.graph_config,
            self.tempo,
            self.bounce_length_beats,
        ) {
            Ok(path) => {
                let format_label = AudioExportFormat::from_path(&path)
                    .map(AudioExportFormat::display_name)
                    .unwrap_or("audio");
                self.status_message = Some(format!(
                    "Offline {format_label} bounce written to {}",
                    path.display()
                ));
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(format!("Offline bounce failed: {err:#}"));
                self.status_message = None;
            }
        }
    }

    fn save_project(&mut self) {
        let result = project_path_from_input(&self.project_path)
            .and_then(|path| self.write_project_to_path(&path));

        match result {
            Ok(path) => {
                self.project_path = path.to_string_lossy().to_string();
                self.status_message = Some(format!("Project saved to {}", path.display()));
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(format!("Failed to save project: {err:#}"));
                self.status_message = None;
            }
        }
    }

    fn open_project(&mut self) {
        let result = project_path_from_input(&self.project_path)
            .and_then(|path| self.read_project_from_path(&path));

        match result {
            Ok(path) => {
                self.project_path = path.to_string_lossy().to_string();
                self.status_message = Some(format!("Opened project {}", path.display()));
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(format!("Failed to open project: {err:#}"));
                self.status_message = None;
            }
        }
    }

    fn write_project_to_path(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let normalized = ensure_hst_extension(path.to_path_buf());
        if let Some(parent) = normalized.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create project directory {}", parent.display())
                })?;
            }
        }

        let document = self.export_project_document();
        let data = serde_json::to_vec_pretty(&document).context("failed to serialise project")?;
        fs::write(&normalized, data)
            .with_context(|| format!("failed to write project file {}", normalized.display()))?;
        Ok(normalized)
    }

    fn read_project_from_path(&mut self, path: &Path) -> anyhow::Result<PathBuf> {
        let normalized = ensure_hst_extension(path.to_path_buf());
        let data = fs::read(&normalized)
            .with_context(|| format!("failed to read project file {}", normalized.display()))?;
        let document: ProjectDocument =
            serde_json::from_slice(&data).context("failed to parse project file")?;
        if document.version > PROJECT_FILE_VERSION {
            anyhow::bail!(
                "project file version {} is newer than supported {}",
                document.version,
                PROJECT_FILE_VERSION
            );
        }
        self.apply_project_document(document)?;
        Ok(normalized)
    }

    fn export_project_document(&self) -> ProjectDocument {
        ProjectDocument {
            version: PROJECT_FILE_VERSION,
            tempo: self.tempo,
            graph_config: self.graph_config.clone(),
            tracks: self.tracks.iter().map(ProjectTrack::from_track).collect(),
            master: ProjectMaster::from_master(&self.master_track),
            next_track_index: self.next_track_index,
            next_clip_index: self.next_clip_index,
            next_color_index: self.next_color_index,
            bounce_path: self.bounce_path.clone(),
            bounce_length_beats: self.bounce_length_beats,
        }
    }

    fn apply_project_document(&mut self, document: ProjectDocument) -> anyhow::Result<()> {
        self.graph_config = document.graph_config;
        self.tracks = document
            .tracks
            .into_iter()
            .map(ProjectTrack::into_track)
            .collect();
        if let Some(first_track) = self.tracks.get(0) {
            self.selected_track = Some(0);
            self.selected_clip = if first_track.clips.is_empty() {
                None
            } else {
                Some((0, 0))
            };
        } else {
            self.selected_track = None;
            self.selected_clip = None;
        }
        self.master_track = document.master.into_master();
        self.next_track_index = document.next_track_index.max(self.tracks.len());
        self.next_clip_index = document.next_clip_index;
        self.next_color_index = document.next_color_index;
        self.bounce_path = document.bounce_path;
        self.bounce_length_beats = document.bounce_length_beats;
        self.tempo = document.tempo;
        self.transport_state = TransportState::Stopped;
        self.stop_all_clips();
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.send_command(EngineCommand::SetTempo(self.tempo));
        Ok(())
    }

    fn add_clip_to_selected_track(&mut self) {
        let Some(track_idx) = self.selected_track else {
            self.last_error = Some("Select a track before adding a clip".into());
            return;
        };

        let clip_number = self.next_clip_index + 1;
        let color = self.next_color();
        self.next_clip_index += 1;

        if let Some(track) = self.tracks.get_mut(track_idx) {
            let start = track.next_clip_start();
            let clip_name = format!("Clip {}", clip_number);
            track.add_clip(Clip::new(clip_name, start, 4.0, color, Vec::new()));
            let clip_idx = track.clips.len() - 1;
            self.selected_clip = Some((track_idx, clip_idx));
        }
    }

    fn add_automation_lane_to_selected_track(&mut self) {
        let Some(track_idx) = self.selected_track else {
            self.last_error = Some("Select a track before adding automation".into());
            return;
        };

        let color = self.next_color();
        let mut status = None;

        if let Some(track) = self.tracks.get_mut(track_idx) {
            let lane_name = format!("Automation {}", track.automation_lanes.len() + 1);
            let default_value = track
                .automation_lanes
                .last()
                .and_then(|lane| lane.points.last().map(|point| point.value))
                .unwrap_or(0.5);
            let points = vec![
                AutomationPoint::new(0.0, default_value),
                AutomationPoint::new(4.0, default_value),
            ];
            track.add_automation_lane(AutomationLane::new(lane_name.clone(), color, points));
            status = Some(format!(
                "Added automation lane '{lane_name}' on {}",
                track.name
            ));
        }

        if let Some(status) = status {
            self.status_message = Some(status);
            self.last_error = None;
        }
    }

    fn draw_transport_toolbar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        egui::Frame::none()
            .fill(palette.toolbar)
            .stroke(Stroke::new(1.0, palette.toolbar_outline))
            .rounding(Rounding::same(20.0))
            .inner_margin(Margin::symmetric(20.0, 16.0))
            .outer_margin(Margin::symmetric(4.0, 0.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(16.0, 12.0);

                    let playing = matches!(self.transport_state, TransportState::Playing);
                    let play_icon = if playing {
                        &self.icons.pause
                    } else {
                        &self.icons.play
                    };
                    let play_label = if playing { "Pause" } else { "Play" };
                    if self
                        .gradient_icon_button(
                            ui,
                            play_icon,
                            play_label,
                            (palette.accent, palette.accent_soft),
                            playing,
                            Vec2::new(150.0, 48.0),
                        )
                        .clicked()
                    {
                        self.toggle_transport();
                    }

                    if self
                        .gradient_icon_button(
                            ui,
                            &self.icons.stop,
                            "Stop",
                            (palette.warning, palette.accent_soft),
                            matches!(self.transport_state, TransportState::Stopped),
                            Vec2::new(120.0, 48.0),
                        )
                        .clicked()
                    {
                        self.stop_transport();
                    }

                    ui.separator();

                    ui.vertical(|ui| {
                        self.section_label(ui, &self.icons.tempo, "Tempo");
                        let tempo_response = ui.add(
                            egui::DragValue::new(&mut self.tempo)
                                .clamp_range(40.0..=220.0)
                                .speed(0.5)
                                .suffix(" BPM"),
                        );
                        if tempo_response.changed() {
                            self.send_command(EngineCommand::SetTempo(self.tempo));
                        }
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                        self.section_label(ui, &self.icons.open, "Project");
                        Self::tinted_frame(&palette, ui, palette.panel_alt, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.project_path)
                                    .desired_width(220.0)
                                    .hint_text("my_project.hst"),
                            );
                        });
                        ui.horizontal(|ui| {
                            if self
                                .gradient_icon_button(
                                    ui,
                                    &self.icons.open,
                                    "Open",
                                    (palette.accent_alt, palette.toolbar_highlight),
                                    false,
                                    Vec2::new(124.0, 40.0),
                                )
                                .clicked()
                            {
                                self.open_project();
                            }
                            if self
                                .gradient_icon_button(
                                    ui,
                                    &self.icons.save,
                                    "Save",
                                    (palette.accent, palette.accent_alt),
                                    false,
                                    Vec2::new(124.0, 40.0),
                                )
                                .clicked()
                            {
                                self.save_project();
                            }
                        });
                    });

                    ui.separator();

                    self.draw_audio_settings(ui);

                    ui.separator();

                    ui.vertical(|ui| {
                        self.section_label(ui, &self.icons.bounce, "Offline Bounce");
                        Self::tinted_frame(&palette, ui, palette.panel_alt, |ui| {
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.bounce_path)
                                        .desired_width(220.0)
                                        .hint_text("bounce.wav"),
                                );
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Length")
                                            .color(palette.text_muted)
                                            .small()
                                            .strong(),
                                    );
                                    ui.add_space(8.0);
                                    ui.add(
                                        egui::DragValue::new(&mut self.bounce_length_beats)
                                            .clamp_range(1.0..=256.0)
                                            .speed(1.0)
                                            .suffix(" beats"),
                                    );
                                });
                            });
                        });
                        if self
                            .gradient_icon_button(
                                ui,
                                &self.icons.bounce,
                                "Render Bounce",
                                (palette.success, palette.accent_alt),
                                false,
                                Vec2::new(186.0, 44.0),
                            )
                            .clicked()
                        {
                            self.bounce_project();
                        }
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                        self.section_label(ui, &self.icons.track, "West Coast Lead");
                        if self
                            .gradient_icon_button(
                                ui,
                                &self.icons.track,
                                "Open Editor",
                                (palette.accent_alt, palette.accent_soft),
                                self.westcoast_editor.show,
                                Vec2::new(186.0, 44.0),
                            )
                            .clicked()
                        {
                            self.westcoast_editor.open();
                        }
                        ui.label(
                            RichText::new("Shape the sine lead's timbre and modulation")
                                .color(palette.text_muted)
                                .small(),
                        );
                    });

                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("Signature: 4 / 4")
                            .color(palette.text_muted)
                            .strong(),
                    );
                });
            });
    }

    fn draw_audio_settings(&mut self, ui: &mut egui::Ui) {
        self.audio_settings
            .update_active(self.engine_runner.realtime());
        let palette = self.palette().clone();
        let mut backend_options: Vec<(AudioBackend, String)> = Vec::new();
        for (backend, label) in available_backends() {
            if backend_options
                .iter()
                .all(|(existing, _)| *existing != backend)
            {
                backend_options.push((backend, label));
            }
        }

        self.section_label(ui, &self.icons.settings, "Audio Settings");
        let backend_options = backend_options;
        Self::tinted_frame(&palette, ui, palette.panel_alt, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Audio backend")
                        .color(palette.text_muted)
                        .small()
                        .strong(),
                );
                let mut backend_choice = self.audio_settings.selected_backend;
                let backend_label = self.audio_settings.selected_backend_label(&backend_options);
                egui::ComboBox::from_id_source("audio_backend_selector")
                    .selected_text(backend_label)
                    .show_ui(ui, |ui| {
                        for (backend, label) in &backend_options {
                            ui.selectable_value(&mut backend_choice, *backend, label);
                        }
                    });
                if backend_choice != self.audio_settings.selected_backend {
                    self.audio_settings.set_selected_backend(backend_choice);
                }

                ui.add_space(6.0);
                ui.label(
                    RichText::new("Output device")
                        .color(palette.text_muted)
                        .small()
                        .strong(),
                );
                let mut device_choice = self.audio_settings.selected_device.clone();
                let device_label = self.audio_settings.selected_device_label();
                egui::ComboBox::from_id_source("audio_device_selector")
                    .selected_text(device_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut device_choice, None, "System Default");
                        for device in &self.audio_settings.available_devices {
                            ui.selectable_value(
                                &mut device_choice,
                                Some(device.id.clone()),
                                &device.label,
                            );
                        }
                    });
                if device_choice != self.audio_settings.selected_device {
                    self.audio_settings.set_selected_device(device_choice);
                }

                if let Some(error) = &self.audio_settings.error {
                    ui.add_space(6.0);
                    ui.label(RichText::new(error).color(palette.warning).small());
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Refresh devices").clicked() {
                        self.audio_settings.refresh_devices();
                    }
                    if ui.button("Apply settings").clicked() {
                        self.apply_audio_settings();
                    }
                });

                ui.add_space(6.0);
                if let Some(active_backend) = self.audio_settings.active_backend {
                    let backend_label = self
                        .audio_settings
                        .active_host_label
                        .clone()
                        .unwrap_or_else(|| active_backend.to_string());
                    let device_label = self
                        .audio_settings
                        .active_device_name
                        .clone()
                        .unwrap_or_else(|| "Unknown device".to_string());
                    ui.label(
                        RichText::new(format!("Active: {backend_label} → {device_label}"))
                            .color(palette.text_muted)
                            .small(),
                    );
                    if let Some(id) = &self.audio_settings.active_device_id {
                        ui.label(
                            RichText::new(format!("Device ID: {id}"))
                                .color(palette.text_muted)
                                .small(),
                        );
                    }
                } else {
                    ui.label(
                        RichText::new("Realtime audio disabled; engine running offline")
                            .color(palette.warning)
                            .small(),
                    );
                }
            });
        });
    }

    fn apply_audio_settings(&mut self) {
        let mut runtime = self.engine_runner.runtime_options().clone();
        runtime.set_backend(self.audio_settings.selected_backend);
        runtime.set_output_device(self.audio_settings.selected_device.clone());

        match self.engine_runner.reconfigure_audio(runtime) {
            Ok(()) => {
                self.audio_settings.sync_with_runtime(
                    self.engine_runner.runtime_options(),
                    self.engine_runner.realtime(),
                );
                if let Some(active) = self.engine_runner.realtime() {
                    let backend_label = self
                        .audio_settings
                        .active_host_label
                        .clone()
                        .unwrap_or_else(|| active.backend().to_string());
                    self.status_message = Some(format!(
                        "Audio output switched to {backend_label} on '{}'",
                        active.device_name()
                    ));
                } else {
                    self.status_message = Some("Realtime audio disabled".to_string());
                }
                self.last_error = None;
            }
            Err(err) => {
                let message = format!("Audio configuration failed: {err:#}");
                self.audio_settings.error = Some(err.to_string());
                self.last_error = Some(message);
                self.status_message = None;
            }
        }
    }

    fn draw_playlist(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        ui.heading(RichText::new("Playlist").color(palette.text_primary));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if self
                .gradient_icon_button(
                    ui,
                    &self.icons.track,
                    "Add Track",
                    (palette.accent_alt, palette.accent),
                    false,
                    Vec2::new(148.0, 40.0),
                )
                .clicked()
            {
                self.add_track();
            }
            if self
                .gradient_icon_button(
                    ui,
                    &self.icons.clip,
                    "Add Clip",
                    (palette.accent, palette.accent_soft),
                    false,
                    Vec2::new(138.0, 40.0),
                )
                .clicked()
            {
                self.add_clip_to_selected_track();
            }
            if self
                .gradient_icon_button(
                    ui,
                    &self.icons.automation,
                    "Add Automation",
                    (palette.success, palette.accent_alt),
                    false,
                    Vec2::new(190.0, 40.0),
                )
                .clicked()
            {
                self.add_automation_lane_to_selected_track();
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        enum ClipAction {
            Launch(usize, usize),
            Stop(usize, usize),
        }

        let pixels_per_beat = self.playlist.pixels_per_beat;
        let track_height = self.playlist.track_height;
        let automation_height = self.playlist.automation_lane_height;
        let header_width = 190.0;
        let ruler_height = 28.0;
        let row_gap = 6.0;

        let mut max_position = self.bounce_length_beats.max(16.0);
        for track in &self.tracks {
            for clip in &track.clips {
                max_position = max_position.max(clip.start_beat + clip.length_beats);
            }
            for lane in &track.automation_lanes {
                for point in &lane.points {
                    max_position = max_position.max(point.beat);
                }
            }
        }
        let visible_beats = (max_position.ceil() + 4.0).max(16.0);
        let timeline_width = pixels_per_beat * visible_beats;

        let mut total_height = ruler_height + row_gap;
        for track in &self.tracks {
            total_height += track_height + row_gap;
            for lane in &track.automation_lanes {
                if lane.visible {
                    total_height += automation_height + row_gap;
                }
            }
        }
        total_height = total_height.max(ui.available_height());
        let desired_width = (header_width + timeline_width + 120.0).max(ui.available_width());

        let mut pending_clip_action: Option<ClipAction> = None;
        let mut track_to_select: Option<usize> = None;
        let mut clip_to_select: Option<(usize, usize)> = None;

        egui::ScrollArea::both()
            .id_source("playlist_scroll")
            .show(ui, |ui| {
                let desired_size = egui::vec2(desired_width, total_height);
                let (response, painter) = ui.allocate_painter(desired_size, Sense::click());
                let rect = response.rect;

                let header_column_rect = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + header_width, rect.bottom()),
                );
                let ruler_rect = egui::Rect::from_min_max(
                    egui::pos2(header_column_rect.right(), rect.top()),
                    egui::pos2(rect.right(), rect.top() + ruler_height),
                );
                let timeline_rect = egui::Rect::from_min_max(
                    egui::pos2(header_column_rect.right(), ruler_rect.bottom()),
                    rect.max,
                );

                painter.rect_filled(rect, 10.0, palette.timeline_bg);
                painter.rect_stroke(rect, 10.0, Stroke::new(1.0, palette.timeline_border));
                painter.rect_filled(header_column_rect, 10.0, palette.track_header);
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(header_column_rect.left(), rect.top()),
                        egui::pos2(header_column_rect.right(), ruler_rect.bottom()),
                    ),
                    10.0,
                    palette.timeline_header,
                );
                painter.rect_filled(timeline_rect, 4.0, palette.timeline_bg);
                painter.rect_filled(ruler_rect, 4.0, palette.timeline_header);

                let total_beats = visible_beats as usize;
                for beat in 0..=total_beats {
                    let x = header_column_rect.right() + beat as f32 * pixels_per_beat;
                    let is_measure = beat % 4 == 0;
                    let color = if is_measure {
                        palette.timeline_grid_primary
                    } else {
                        palette.timeline_grid_secondary
                    };
                    painter.line_segment(
                        [
                            egui::pos2(x, timeline_rect.top()),
                            egui::pos2(x, timeline_rect.bottom()),
                        ],
                        Stroke::new(if is_measure { 1.5 } else { 1.0 }, color),
                    );
                    if is_measure {
                        let bar_idx = beat / 4 + 1;
                        painter.text(
                            egui::pos2(x + 6.0, ruler_rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("Bar {bar_idx}"),
                            FontId::proportional(13.0),
                            palette.ruler_text,
                        );
                    }
                }

                let pointer_pos = response.interact_pointer_pos();
                let clicked = response.clicked_by(PointerButton::Primary);
                let double_clicked = response.double_clicked_by(PointerButton::Primary);
                let right_clicked = response.clicked_by(PointerButton::Secondary);

                let mut cursor_y = timeline_rect.top() + row_gap;
                for (track_idx, track) in self.tracks.iter_mut().enumerate() {
                    let track_header_rect = egui::Rect::from_min_max(
                        egui::pos2(header_column_rect.left(), cursor_y),
                        egui::pos2(header_column_rect.right(), cursor_y + track_height),
                    );
                    let track_lane_rect = egui::Rect::from_min_max(
                        egui::pos2(timeline_rect.left(), cursor_y),
                        egui::pos2(timeline_rect.right(), cursor_y + track_height),
                    );
                    let is_selected = self.selected_track == Some(track_idx);

                    let header_fill = if is_selected {
                        palette.track_header_selected
                    } else {
                        palette.track_header
                    };
                    painter.rect_filled(track_header_rect, 6.0, header_fill);
                    painter.rect_stroke(
                        track_header_rect,
                        6.0,
                        Stroke::new(1.0, palette.timeline_border),
                    );

                    if is_selected {
                        painter.rect_filled(track_lane_rect, 0.0, palette.track_lane_overlay);
                    }

                    painter.text(
                        egui::pos2(
                            track_header_rect.left() + 12.0,
                            track_header_rect.center().y,
                        ),
                        Align2::LEFT_CENTER,
                        format!("{:02} {}", track_idx + 1, track.name),
                        FontId::proportional(14.0),
                        palette.text_primary,
                    );

                    let button_size = egui::vec2(28.0, 20.0);
                    let button_gap = 8.0;
                    let mute_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            track_header_rect.right() - button_gap - button_size.x * 2.0,
                            track_header_rect.center().y - button_size.y * 0.5,
                        ),
                        button_size,
                    );
                    let solo_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            track_header_rect.right() - button_gap - button_size.x,
                            track_header_rect.center().y - button_size.y * 0.5,
                        ),
                        button_size,
                    );

                    let mute_color = if track.muted {
                        palette.warning
                    } else {
                        palette.track_button_bg
                    };
                    painter.rect_filled(mute_rect, 4.0, mute_color);
                    painter.rect_stroke(
                        mute_rect,
                        4.0,
                        Stroke::new(1.0, palette.track_button_border),
                    );
                    painter.text(
                        mute_rect.center(),
                        Align2::CENTER_CENTER,
                        "M",
                        FontId::proportional(13.0),
                        palette.text_primary,
                    );

                    let solo_color = if track.solo {
                        palette.success
                    } else {
                        palette.track_button_bg
                    };
                    painter.rect_filled(solo_rect, 4.0, solo_color);
                    painter.rect_stroke(
                        solo_rect,
                        4.0,
                        Stroke::new(1.0, palette.track_button_border),
                    );
                    painter.text(
                        solo_rect.center(),
                        Align2::CENTER_CENTER,
                        "S",
                        FontId::proportional(13.0),
                        palette.text_primary,
                    );

                    if clicked {
                        if let Some(pos) = pointer_pos {
                            if mute_rect.contains(pos) {
                                track.muted = !track.muted;
                            } else if solo_rect.contains(pos) {
                                track.solo = !track.solo;
                            } else if track_header_rect.contains(pos) {
                                track_to_select = Some(track_idx);
                            }
                        }
                    }

                    let clip_y = track_lane_rect.top() + 6.0;
                    let clip_height = track_lane_rect.height() - 12.0;
                    for (clip_idx, clip) in track.clips.iter_mut().enumerate() {
                        let clip_start = track_lane_rect.left() + clip.start_beat * pixels_per_beat;
                        let clip_width = clip.length_beats.max(0.25) * pixels_per_beat;
                        let clip_rect = egui::Rect::from_min_max(
                            egui::pos2(clip_start, clip_y),
                            egui::pos2(clip_start + clip_width, clip_y + clip_height),
                        );
                        let clip_selected = self.selected_clip == Some((track_idx, clip_idx));
                        let clip_hovered = pointer_pos
                            .map(|pos| clip_rect.contains(pos))
                            .unwrap_or(false);

                        let shadow_rect = clip_rect.translate(Vec2::new(0.0, 3.0));
                        painter.rect_filled(shadow_rect, 8.0, palette.clip_shadow);

                        let mut fill = if clip_selected {
                            clip.color
                        } else {
                            clip.color.gamma_multiply(0.75)
                        };
                        if clip_hovered {
                            fill = fill.gamma_multiply(1.1);
                        }
                        if clip.launch_state.is_playing() {
                            fill = fill.gamma_multiply(1.2);
                        }

                        painter.rect_filled(clip_rect, 6.0, fill);
                        let border_color = if clip.launch_state.is_playing() {
                            palette.clip_border_playing
                        } else if clip_selected {
                            palette.clip_border_active
                        } else {
                            palette.clip_border_default
                        };
                        painter.rect_stroke(clip_rect, 6.0, Stroke::new(1.5, border_color));

                        painter.text(
                            egui::pos2(clip_rect.left() + 10.0, clip_rect.center().y),
                            Align2::LEFT_CENTER,
                            clip.name.as_str(),
                            FontId::proportional(13.0),
                            palette.clip_text_primary,
                        );
                        painter.text(
                            egui::pos2(clip_rect.right() - 10.0, clip_rect.bottom() - 8.0),
                            Align2::RIGHT_BOTTOM,
                            format!("{:.1} – {:.1}", clip.start_beat, clip.length_beats),
                            FontId::proportional(11.0),
                            palette.clip_text_secondary,
                        );
                        if clip.launch_state.is_playing() {
                            painter.text(
                                egui::pos2(clip_rect.left() + 10.0, clip_rect.top() + 6.0),
                                Align2::LEFT_TOP,
                                "▶",
                                FontId::proportional(12.0),
                                palette.clip_text_primary,
                            );
                        }

                        if clip_hovered && clicked {
                            clip_to_select = Some((track_idx, clip_idx));
                            track_to_select = Some(track_idx);
                        }
                        if clip_hovered && double_clicked {
                            if clip.launch_state.is_playing() {
                                pending_clip_action = Some(ClipAction::Stop(track_idx, clip_idx));
                            } else {
                                pending_clip_action = Some(ClipAction::Launch(track_idx, clip_idx));
                            }
                        }
                    }

                    cursor_y += track_height + row_gap;

                    for lane in track.automation_lanes.iter_mut() {
                        let lane_header_rect = egui::Rect::from_min_max(
                            egui::pos2(header_column_rect.left(), cursor_y),
                            egui::pos2(header_column_rect.right(), cursor_y + automation_height),
                        );
                        let lane_rect = egui::Rect::from_min_max(
                            egui::pos2(timeline_rect.left(), cursor_y),
                            egui::pos2(timeline_rect.right(), cursor_y + automation_height),
                        );

                        let header_fill = if lane.visible {
                            palette.automation_header
                        } else {
                            palette.automation_header_muted
                        };
                        painter.rect_filled(lane_header_rect, 6.0, header_fill);
                        painter.rect_stroke(
                            lane_header_rect,
                            6.0,
                            Stroke::new(1.0, palette.timeline_border),
                        );
                        painter.text(
                            egui::pos2(lane_header_rect.left() + 12.0, lane_header_rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("Automation {}", lane.parameter()),
                            FontId::proportional(12.0),
                            lane.color(),
                        );

                        let lane_button_size = egui::vec2(32.0, 18.0);
                        let lane_button_gap = 6.0;
                        let visibility_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                lane_header_rect.right() - lane_button_gap - lane_button_size.x,
                                lane_header_rect.center().y - lane_button_size.y * 0.5,
                            ),
                            lane_button_size,
                        );
                        let add_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                visibility_rect.left() - lane_button_gap - lane_button_size.x,
                                lane_header_rect.center().y - lane_button_size.y * 0.5,
                            ),
                            lane_button_size,
                        );

                        let add_color = lane.color().gamma_multiply(0.6);
                        painter.rect_filled(add_rect, 4.0, add_color);
                        painter.rect_stroke(
                            add_rect,
                            4.0,
                            Stroke::new(1.0, palette.track_button_border),
                        );
                        painter.text(
                            add_rect.center(),
                            Align2::CENTER_CENTER,
                            "+",
                            FontId::proportional(14.0),
                            palette.text_primary,
                        );

                        let visibility_color = if lane.visible {
                            lane.color().gamma_multiply(0.85)
                        } else {
                            palette.track_button_bg
                        };
                        painter.rect_filled(visibility_rect, 4.0, visibility_color);
                        painter.rect_stroke(
                            visibility_rect,
                            4.0,
                            Stroke::new(1.0, palette.track_button_border),
                        );
                        painter.text(
                            visibility_rect.center(),
                            Align2::CENTER_CENTER,
                            if lane.visible { "On" } else { "Off" },
                            FontId::proportional(11.0),
                            palette.text_primary,
                        );

                        if lane.visible {
                            painter.rect_filled(lane_rect, 6.0, palette.automation_lane_bg);
                            let overlay = Color32::from_rgba_unmultiplied(
                                lane.color().r(),
                                lane.color().g(),
                                lane.color().b(),
                                28,
                            );
                            painter.rect_filled(lane_rect, 6.0, overlay);
                            painter.rect_stroke(
                                lane_rect,
                                6.0,
                                Stroke::new(1.0, palette.timeline_border),
                            );

                            let mut last_point_pos: Option<egui::Pos2> = None;
                            let mut remove_point: Option<usize> = None;
                            let vertical_padding = 8.0;
                            let usable_height = lane_rect.height() - vertical_padding * 2.0;
                            for (point_idx, point) in lane.points.iter().enumerate() {
                                let x = lane_rect.left() + point.beat * pixels_per_beat;
                                let value = point.value.clamp(0.0, 1.0);
                                let y =
                                    lane_rect.bottom() - vertical_padding - value * usable_height;
                                let pos = egui::pos2(x, y);
                                if let Some(prev) = last_point_pos {
                                    painter
                                        .line_segment([prev, pos], Stroke::new(1.5, lane.color()));
                                }
                                last_point_pos = Some(pos);
                                painter.circle_filled(pos, 4.0, lane.color());
                                painter.circle_stroke(
                                    pos,
                                    4.0,
                                    Stroke::new(1.0, palette.automation_point_border),
                                );
                                if right_clicked {
                                    if let Some(pointer) = pointer_pos {
                                        if pointer.distance(pos) <= 8.0 && lane.points.len() > 1 {
                                            remove_point = Some(point_idx);
                                        }
                                    }
                                }
                            }
                            if let Some(idx) = remove_point {
                                lane.points.remove(idx);
                            }
                            if double_clicked {
                                if let Some(pointer) = pointer_pos {
                                    if lane_rect.contains(pointer) {
                                        let beat = ((pointer.x - lane_rect.left())
                                            / pixels_per_beat)
                                            .max(0.0);
                                        let value = ((lane_rect.bottom() - pointer.y)
                                            / lane_rect.height())
                                        .clamp(0.0, 1.0);
                                        lane.add_point(AutomationPoint::new(beat, value));
                                    }
                                }
                            }
                        } else {
                            painter.rect_filled(lane_rect, 6.0, palette.automation_lane_hidden_bg);
                            painter.rect_stroke(
                                lane_rect,
                                6.0,
                                Stroke::new(1.0, palette.timeline_border),
                            );
                            painter.text(
                                lane_rect.center(),
                                Align2::CENTER_CENTER,
                                "Automation lane hidden",
                                FontId::proportional(11.0),
                                palette.text_muted,
                            );
                        }

                        if clicked {
                            if let Some(pos) = pointer_pos {
                                if add_rect.contains(pos) {
                                    let template_point = lane
                                        .points
                                        .last()
                                        .cloned()
                                        .unwrap_or_else(|| AutomationPoint::new(0.0, 0.5));
                                    lane.add_point(AutomationPoint::new(
                                        template_point.beat + 1.0,
                                        template_point.value,
                                    ));
                                } else if visibility_rect.contains(pos) {
                                    lane.visible = !lane.visible;
                                } else if lane_rect.contains(pos) || lane_header_rect.contains(pos)
                                {
                                    track_to_select = Some(track_idx);
                                    clip_to_select = None;
                                }
                            }
                        }

                        cursor_y += automation_height + row_gap;
                    }
                }
            });

        if let Some((track_idx, clip_idx)) = clip_to_select {
            self.selected_clip = Some((track_idx, clip_idx));
            self.selected_track = Some(track_idx);
        } else if let Some(track_idx) = track_to_select {
            if self.selected_track != Some(track_idx) {
                self.selected_clip = None;
            }
            self.selected_track = Some(track_idx);
        }

        if let Some(action) = pending_clip_action {
            match action {
                ClipAction::Launch(track_idx, clip_idx) => self.launch_clip(track_idx, clip_idx),
                ClipAction::Stop(track_idx, clip_idx) => self.stop_clip(track_idx, clip_idx),
            }
        }
    }

    fn draw_piano_roll(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        ui.heading(RichText::new("Piano Roll").color(palette.text_primary));
        ui.add_space(6.0);

        if let Some((track_idx, clip_idx)) = self.selected_clip {
            if let Some(track) = self.tracks.get_mut(track_idx) {
                if let Some(clip) = track.clips.get_mut(clip_idx) {
                    let desired_size = egui::vec2(ui.available_width(), ui.available_height());
                    let (response, painter) =
                        ui.allocate_painter(desired_size, egui::Sense::click_and_drag());

                    let rect = response.rect;
                    let key_height = self.piano_roll.key_height;
                    let pixels_per_beat = self.piano_roll.pixels_per_beat;
                    let num_keys = self.piano_roll.key_range_len();

                    // Background grid
                    painter.rect_filled(rect, 8.0, palette.piano_background);

                    for i in 0..=num_keys {
                        let y = rect.bottom() - key_height * i as f32;
                        painter.line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(1.0, palette.piano_grid_minor),
                        );
                    }

                    let total_beats = clip.length_beats.max(1.0).ceil() as usize;
                    for beat in 0..=total_beats * 4 {
                        let x = rect.left() + beat as f32 * pixels_per_beat / 4.0;
                        let color = if beat % 4 == 0 {
                            palette.piano_grid_major
                        } else {
                            palette.piano_grid_minor
                        };
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            egui::Stroke::new(1.0, color),
                        );
                    }

                    // Draw notes
                    for note in &clip.notes {
                        if !self.piano_roll.is_pitch_visible(note.pitch) {
                            continue;
                        }
                        let x = rect.left() + note.start_beats * pixels_per_beat;
                        let width = note.length_beats * pixels_per_beat;
                        let y = self.piano_roll.pitch_to_y(rect, note.pitch);
                        let note_rect = egui::Rect::from_min_size(
                            egui::pos2(x, y - key_height + 2.0),
                            egui::vec2(width.max(10.0), key_height - 4.0),
                        );
                        painter.rect_filled(note_rect, 4.0, clip.color.gamma_multiply(1.2));
                        painter.rect_stroke(
                            note_rect,
                            4.0,
                            egui::Stroke::new(1.0, palette.timeline_border),
                        );
                    }

                    if response.double_clicked() {
                        if let Some(pointer_pos) = response.interact_pointer_pos() {
                            let beat = ((pointer_pos.x - rect.left()) / pixels_per_beat).max(0.0);
                            let pitch = self
                                .piano_roll
                                .y_to_pitch(rect, pointer_pos.y)
                                .unwrap_or(self.piano_roll.key_range.end().saturating_sub(12));
                            clip.notes.push(Note::new(beat, 1.0, pitch));
                        }
                    }

                    return;
                }
            }
        }

        ui.label("Select a clip to edit its notes.");
    }

    fn update_mixer_visuals(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        let any_solo = self.tracks.iter().any(|track| track.solo);
        let transport_playing = matches!(self.transport_state, TransportState::Playing);
        for (index, track) in self.tracks.iter_mut().enumerate() {
            track.update_meter(time, index, transport_playing, any_solo);
        }
        self.master_track.update_from_tracks(&self.tracks);
    }

    fn draw_meter(ui: &mut egui::Ui, meter: &TrackMeter, palette: &HarmoniqPalette) {
        let desired_size = egui::vec2(32.0, 120.0);
        let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, palette.meter_background);
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, palette.meter_border));

        let gutter = 3.0;
        let bar_width = (rect.width() - gutter * 3.0) / 2.0;
        let max_height = rect.height() - gutter * 2.0;
        let left_height = meter.left_level().clamp(0.0, 1.0) * max_height;
        let right_height = meter.right_level().clamp(0.0, 1.0) * max_height;

        let left_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + gutter, rect.bottom() - gutter - left_height),
            egui::pos2(rect.left() + gutter + bar_width, rect.bottom() - gutter),
        );
        let right_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.right() - gutter - bar_width,
                rect.bottom() - gutter - right_height,
            ),
            egui::pos2(rect.right() - gutter, rect.bottom() - gutter),
        );

        painter.rect_filled(left_rect, 3.0, palette.meter_left);
        painter.rect_filled(right_rect, 3.0, palette.meter_right);

        let rms_height = meter.rms_level().clamp(0.0, 1.0) * max_height;
        let rms_y = rect.bottom() - gutter - rms_height;
        painter.line_segment(
            [
                egui::pos2(rect.left() + gutter, rms_y),
                egui::pos2(rect.right() - gutter, rms_y),
            ],
            Stroke::new(1.0, palette.meter_rms),
        );
    }

    fn draw_effects_ui(
        effects: &mut Vec<MixerEffect>,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
    ) {
        if effects.is_empty() {
            ui.label(
                RichText::new("No effects loaded")
                    .italics()
                    .color(palette.text_muted),
            );
        }

        let mut removal: Option<usize> = None;
        for (index, effect) in effects.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(effect.effect_type.display_name()).strong());
                    let toggle_label = if effect.enabled { "Bypass" } else { "Enable" };
                    if ui.button(toggle_label).clicked() {
                        effect.enabled = !effect.enabled;
                    }
                    if ui.button("Remove").clicked() {
                        removal = Some(index);
                    }
                });
                ui.label(
                    RichText::new(effect.effect_type.identifier())
                        .small()
                        .color(palette.text_muted),
                );
            });
        }

        if let Some(index) = removal {
            effects.remove(index);
        }

        ui.menu_button(RichText::new("+ Add Effect").strong(), |ui| {
            for effect_type in EffectType::all() {
                if ui.button(effect_type.display_name()).clicked() {
                    effects.push(MixerEffect::new(*effect_type));
                    ui.close_menu();
                }
            }
        });
    }

    fn draw_track_strip(
        ui: &mut egui::Ui,
        index: usize,
        track: &mut Track,
        is_selected: bool,
        palette: &HarmoniqPalette,
    ) -> bool {
        let fill = if track.muted {
            palette.mixer_strip_muted
        } else if track.solo {
            palette.mixer_strip_solo
        } else if is_selected {
            palette.mixer_strip_selected
        } else {
            palette.mixer_strip_bg
        };
        let mut frame = egui::Frame::group(ui.style());
        frame.fill = fill;
        frame.stroke = Stroke::new(1.0, palette.mixer_strip_border);
        let mut clicked = false;
        frame.show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.vertical(|ui| {
                let label = format!("{:02} {}", index + 1, track.name);
                if ui
                    .selectable_label(is_selected, RichText::new(label).strong())
                    .clicked()
                {
                    clicked = true;
                }
                ui.add_space(6.0);
                Self::draw_meter(ui, &track.meter, palette);
                ui.add_space(6.0);
                ui.add(Knob::new(&mut track.volume, 0.0, 1.5, 0.9, "Vol", palette));
                ui.add(Knob::new(&mut track.pan, -1.0, 1.0, 0.0, "Pan", palette));
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut track.muted, "Mute");
                    ui.toggle_value(&mut track.solo, "Solo");
                });
                ui.label(
                    RichText::new(format!("{:.1} dB", track.meter.level_db()))
                        .small()
                        .color(palette.text_muted),
                );
                ui.separator();
                ui.label(
                    RichText::new("Effects")
                        .strong()
                        .small()
                        .color(palette.text_muted),
                );
                Self::draw_effects_ui(&mut track.effects, ui, palette);
            });
        });
        ui.add_space(8.0);
        clicked
    }

    fn draw_master_strip(ui: &mut egui::Ui, master: &mut MasterChannel, palette: &HarmoniqPalette) {
        let mut frame = egui::Frame::group(ui.style());
        frame.fill = palette.mixer_strip_selected;
        frame.stroke = Stroke::new(1.0, palette.mixer_strip_border);
        frame.show(ui, |ui| {
            ui.set_min_width(170.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&master.name)
                        .strong()
                        .color(palette.text_primary),
                );
                ui.add_space(6.0);
                Self::draw_meter(ui, &master.meter, palette);
                ui.add_space(6.0);
                ui.add(Knob::new(&mut master.volume, 0.0, 1.5, 1.0, "Vol", palette));
                ui.label(
                    RichText::new(format!("{:.1} dB", master.meter.level_db()))
                        .small()
                        .color(palette.text_muted),
                );
                ui.separator();
                ui.label(
                    RichText::new("Master Effects")
                        .strong()
                        .small()
                        .color(palette.text_muted),
                );
                Self::draw_effects_ui(&mut master.effects, ui, palette);
            });
        });
    }

    fn draw_mixer(&mut self, ui: &mut egui::Ui) {
        self.update_mixer_visuals(ui.ctx());
        let palette = self.palette().clone();
        ui.heading(RichText::new("Mixer").color(palette.text_primary));
        ui.add_space(6.0);
        let mut new_selection = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, track) in self.tracks.iter_mut().enumerate() {
                    if Self::draw_track_strip(
                        ui,
                        index,
                        track,
                        self.selected_track == Some(index),
                        &palette,
                    ) {
                        new_selection = Some(index);
                    }
                }
                Self::draw_master_strip(ui, &mut self.master_track, &palette);
            });
        });
        if let Some(selection) = new_selection {
            self.selected_track = Some(selection);
        }
    }
}

impl App for HarmoniqStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let palette = self.palette().clone();

        let keyboard_events = self.typing_keyboard.collect_midi_events(ctx);
        if !keyboard_events.is_empty() {
            self.send_command(EngineCommand::SubmitMidi(keyboard_events));
        }

        egui::TopBottomPanel::top("transport")
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .outer_margin(Margin::symmetric(12.0, 10.0)),
            )
            .show(ctx, |ui| self.draw_transport_toolbar(ui));

        egui::SidePanel::left("playlist")
            .resizable(true)
            .default_width(280.0)
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(18.0, 16.0))
                    .outer_margin(Margin::symmetric(12.0, 8.0))
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(18.0)),
            )
            .show(ctx, |ui| self.draw_playlist(ui));

        egui::TopBottomPanel::bottom("mixer")
            .resizable(true)
            .default_height(240.0)
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(18.0, 14.0))
                    .outer_margin(Margin::symmetric(12.0, 10.0))
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(18.0)),
            )
            .show(ctx, |ui| self.draw_mixer(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(18.0, 16.0))
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(20.0)),
            )
            .show(ctx, |ui| self.draw_piano_roll(ui));

        self.westcoast_editor.draw(ctx, &palette);

        if let Some(message) = self.status_message.clone() {
            let mut clear_message = false;
            egui::Window::new("Status")
                .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -16.0])
                .collapsible(false)
                .frame(
                    egui::Frame::none()
                        .fill(palette.panel)
                        .stroke(Stroke::new(1.0, palette.toolbar_outline))
                        .rounding(Rounding::same(16.0))
                        .inner_margin(Margin::symmetric(12.0, 10.0)),
                )
                .show(ctx, |ui| {
                    ui.label(RichText::new(message).color(palette.text_primary));
                    if ui.button("Dismiss").clicked() {
                        clear_message = true;
                    }
                });
            if clear_message {
                self.status_message = None;
            }
        }

        if let Some(error) = &self.last_error {
            egui::Window::new("Engine Warnings")
                .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
                .collapsible(false)
                .frame(
                    egui::Frame::none()
                        .fill(palette.panel)
                        .stroke(Stroke::new(1.0, palette.toolbar_outline))
                        .rounding(Rounding::same(16.0))
                        .inner_margin(Margin::symmetric(12.0, 10.0)),
                )
                .show(ctx, |ui| {
                    ui.label(RichText::new(error).color(palette.warning));
                });
        }
    }
}

struct Track {
    name: String,
    clips: Vec<Clip>,
    automation_lanes: Vec<AutomationLane>,
    volume: f32,
    pan: f32,
    muted: bool,
    solo: bool,
    effects: Vec<MixerEffect>,
    meter: TrackMeter,
}

impl Track {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
            automation_lanes: Vec::new(),
            volume: 0.9,
            pan: 0.0,
            muted: false,
            solo: false,
            effects: Vec::new(),
            meter: TrackMeter::default(),
        }
    }

    fn with_index(index: usize) -> Self {
        Self::new(format!("Track {index:02}"))
    }

    fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
    }

    fn add_automation_lane(&mut self, lane: AutomationLane) {
        self.automation_lanes.push(lane);
    }

    fn next_clip_start(&self) -> f32 {
        self.clips
            .iter()
            .map(|clip| clip.start_beat + clip.length_beats)
            .fold(0.0, f32::max)
    }

    fn stop_all_clips(&mut self) {
        for clip in &mut self.clips {
            clip.launch_state = ClipLaunchState::Stopped;
        }
    }

    fn has_playing_clips(&self) -> bool {
        self.clips.iter().any(|clip| clip.launch_state.is_playing())
    }

    fn update_meter(&mut self, time: f64, index: usize, transport_playing: bool, any_solo: bool) {
        let mut activity = if transport_playing {
            if self.has_playing_clips() {
                1.0
            } else {
                0.4
            }
        } else {
            0.15
        };
        if self.muted {
            activity *= 0.1;
        }
        if any_solo && !self.solo {
            activity *= 0.1;
        }
        let lfo = ((time + index as f64 * 0.37).sin() as f32 * 0.5 + 0.5).powf(0.8);
        let level = (self.volume * activity * lfo).clamp(0.0, 1.5);
        let pan = self.pan.clamp(-1.0, 1.0);
        let left_weight = (1.0 - pan) * 0.5;
        let right_weight = (1.0 + pan) * 0.5;
        let left = (level * left_weight).clamp(0.0, 1.0);
        let right = (level * right_weight).clamp(0.0, 1.0);
        self.meter.update(left, right);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum EffectType {
    ParametricEq,
    Compressor,
    Limiter,
    Reverb,
    Delay,
    Chorus,
    Flanger,
    Phaser,
    Distortion,
    AutoFilter,
    StereoEnhancer,
    NoiseGate,
}

impl EffectType {
    fn all() -> &'static [EffectType] {
        &[
            EffectType::ParametricEq,
            EffectType::Compressor,
            EffectType::Limiter,
            EffectType::Reverb,
            EffectType::Delay,
            EffectType::Chorus,
            EffectType::Flanger,
            EffectType::Phaser,
            EffectType::Distortion,
            EffectType::AutoFilter,
            EffectType::StereoEnhancer,
            EffectType::NoiseGate,
        ]
    }

    fn display_name(&self) -> &'static str {
        match self {
            EffectType::ParametricEq => "Parametric EQ",
            EffectType::Compressor => "Compressor",
            EffectType::Limiter => "Limiter",
            EffectType::Reverb => "Reverb",
            EffectType::Delay => "Delay / Echo",
            EffectType::Chorus => "Chorus",
            EffectType::Flanger => "Flanger",
            EffectType::Phaser => "Phaser",
            EffectType::Distortion => "Distortion / Saturation",
            EffectType::AutoFilter => "Filter / Auto Filter",
            EffectType::StereoEnhancer => "Stereo Enhancer",
            EffectType::NoiseGate => "Noise Gate / Expander",
        }
    }

    fn identifier(&self) -> &'static str {
        match self {
            EffectType::ParametricEq => "harmoniq.effects.parametric_eq",
            EffectType::Compressor => "harmoniq.effects.compressor",
            EffectType::Limiter => "harmoniq.effects.limiter",
            EffectType::Reverb => "harmoniq.effects.reverb",
            EffectType::Delay => "harmoniq.effects.delay",
            EffectType::Chorus => "harmoniq.effects.chorus",
            EffectType::Flanger => "harmoniq.effects.flanger",
            EffectType::Phaser => "harmoniq.effects.phaser",
            EffectType::Distortion => "harmoniq.effects.distortion",
            EffectType::AutoFilter => "harmoniq.effects.autofilter",
            EffectType::StereoEnhancer => "harmoniq.effects.stereo_enhancer",
            EffectType::NoiseGate => "harmoniq.effects.noise_gate",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MixerEffect {
    effect_type: EffectType,
    enabled: bool,
}

impl MixerEffect {
    fn new(effect_type: EffectType) -> Self {
        Self {
            effect_type,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
struct TrackMeter {
    left: f32,
    right: f32,
    rms: f32,
}

impl Default for TrackMeter {
    fn default() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            rms: 0.0,
        }
    }
}

impl TrackMeter {
    fn update(&mut self, left: f32, right: f32) {
        self.left = self.left * 0.6 + left * 0.4;
        self.right = self.right * 0.6 + right * 0.4;
        let rms = ((left * left + right * right) * 0.5).sqrt();
        self.rms = self.rms * 0.7 + rms * 0.3;
    }

    fn level_db(&self) -> f32 {
        20.0 * self.rms.max(1e-4).log10()
    }

    fn left_level(&self) -> f32 {
        self.left
    }

    fn right_level(&self) -> f32 {
        self.right
    }

    fn rms_level(&self) -> f32 {
        self.rms
    }
}

#[derive(Debug, Clone)]
struct MasterChannel {
    name: String,
    volume: f32,
    meter: TrackMeter,
    effects: Vec<MixerEffect>,
}

impl Default for MasterChannel {
    fn default() -> Self {
        Self {
            name: "Master".into(),
            volume: 1.0,
            meter: TrackMeter::default(),
            effects: vec![
                MixerEffect::new(EffectType::ParametricEq),
                MixerEffect::new(EffectType::Limiter),
            ],
        }
    }
}

impl MasterChannel {
    fn update_from_tracks(&mut self, tracks: &[Track]) {
        if tracks.is_empty() {
            self.meter.update(0.0, 0.0);
            return;
        }
        let mut left = 0.0;
        let mut right = 0.0;
        for track in tracks {
            left += track.meter.left;
            right += track.meter.right;
        }
        let count = tracks.len() as f32;
        let scaled_left = (left / count) * self.volume;
        let scaled_right = (right / count) * self.volume;
        self.meter
            .update(scaled_left.clamp(0.0, 1.0), scaled_right.clamp(0.0, 1.0));
    }
}

const PROJECT_FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectDocument {
    version: u32,
    tempo: f32,
    graph_config: DemoGraphConfig,
    tracks: Vec<ProjectTrack>,
    master: ProjectMaster,
    next_track_index: usize,
    next_clip_index: usize,
    next_color_index: usize,
    bounce_path: String,
    bounce_length_beats: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectTrack {
    name: String,
    volume: f32,
    pan: f32,
    muted: bool,
    solo: bool,
    effects: Vec<MixerEffect>,
    clips: Vec<ProjectClip>,
    automation_lanes: Vec<ProjectAutomationLane>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectClip {
    name: String,
    start_beat: f32,
    length_beats: f32,
    color: RgbaColor,
    notes: Vec<Note>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectAutomationLane {
    parameter: String,
    color: RgbaColor,
    visible: bool,
    points: Vec<AutomationPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectMaster {
    name: String,
    volume: f32,
    effects: Vec<MixerEffect>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct RgbaColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl From<Color32> for RgbaColor {
    fn from(color: Color32) -> Self {
        let [r, g, b, a] = color.to_array();
        Self { r, g, b, a }
    }
}

impl From<RgbaColor> for Color32 {
    fn from(color: RgbaColor) -> Self {
        Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
    }
}

fn is_hst_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("hst"))
        .unwrap_or(false)
}

fn ensure_hst_extension(mut path: PathBuf) -> PathBuf {
    if !is_hst_extension(&path) {
        path.set_extension("hst");
    }
    path
}

fn project_path_from_input(input: &str) -> anyhow::Result<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("project path cannot be empty");
    }
    Ok(ensure_hst_extension(PathBuf::from(trimmed)))
}

impl ProjectTrack {
    fn from_track(track: &Track) -> Self {
        Self {
            name: track.name.clone(),
            volume: track.volume,
            pan: track.pan,
            muted: track.muted,
            solo: track.solo,
            effects: track.effects.clone(),
            clips: track.clips.iter().map(ProjectClip::from_clip).collect(),
            automation_lanes: track
                .automation_lanes
                .iter()
                .map(ProjectAutomationLane::from_lane)
                .collect(),
        }
    }

    fn into_track(self) -> Track {
        let mut track = Track::new(self.name);
        track.volume = self.volume;
        track.pan = self.pan;
        track.muted = self.muted;
        track.solo = self.solo;
        track.effects = self.effects;
        track.clips = self.clips.into_iter().map(ProjectClip::into_clip).collect();
        track.automation_lanes = self
            .automation_lanes
            .into_iter()
            .map(ProjectAutomationLane::into_lane)
            .collect();
        track
    }
}

impl ProjectClip {
    fn from_clip(clip: &Clip) -> Self {
        Self {
            name: clip.name.clone(),
            start_beat: clip.start_beat,
            length_beats: clip.length_beats,
            color: clip.color.into(),
            notes: clip.notes.clone(),
        }
    }

    fn into_clip(self) -> Clip {
        Clip {
            name: self.name,
            start_beat: self.start_beat,
            length_beats: self.length_beats,
            color: self.color.into(),
            notes: self.notes,
            launch_state: ClipLaunchState::Stopped,
        }
    }
}

impl ProjectAutomationLane {
    fn from_lane(lane: &AutomationLane) -> Self {
        Self {
            parameter: lane.parameter.clone(),
            color: lane.color.into(),
            visible: lane.visible,
            points: lane.points.clone(),
        }
    }

    fn into_lane(self) -> AutomationLane {
        let mut lane = AutomationLane::new(self.parameter, self.color.into(), self.points);
        lane.visible = self.visible;
        lane
    }
}

impl ProjectMaster {
    fn from_master(master: &MasterChannel) -> Self {
        Self {
            name: master.name.clone(),
            volume: master.volume,
            effects: master.effects.clone(),
        }
    }

    fn into_master(self) -> MasterChannel {
        MasterChannel {
            name: self.name,
            volume: self.volume,
            meter: TrackMeter::default(),
            effects: self.effects,
        }
    }
}

struct Knob<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    default: f32,
    label: &'a str,
    palette: &'a HarmoniqPalette,
}

impl<'a> Knob<'a> {
    fn new(
        value: &'a mut f32,
        min: f32,
        max: f32,
        default: f32,
        label: &'a str,
        palette: &'a HarmoniqPalette,
    ) -> Self {
        Self {
            value,
            min,
            max,
            default,
            label,
            palette,
        }
    }
}

impl<'a> egui::Widget for Knob<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = egui::vec2(64.0, 80.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::drag());
        let mut value = (*self.value).clamp(self.min, self.max);

        if response.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta().y);
            let sensitivity = (self.max - self.min).abs() / 160.0;
            value -= delta * sensitivity;
            value = value.clamp(self.min, self.max);
            *self.value = value;
            response.mark_changed();
            ui.ctx().request_repaint();
        } else {
            *self.value = value;
        }

        if response.double_clicked() {
            *self.value = self.default.clamp(self.min, self.max);
            response.mark_changed();
        }

        let knob_radius = 22.0;
        let knob_center = egui::pos2(rect.center().x, rect.top() + knob_radius + 6.0);
        let painter = ui.painter_at(rect);
        painter.circle_filled(knob_center, knob_radius, self.palette.knob_base);
        painter.circle_stroke(
            knob_center,
            knob_radius,
            Stroke::new(2.0, self.palette.knob_ring),
        );

        let normalized = (value - self.min) / (self.max - self.min).max(1e-6);
        let angle = (-135.0_f32.to_radians()) + normalized * (270.0_f32.to_radians());
        let indicator = egui::pos2(
            knob_center.x + angle.cos() * (knob_radius - 6.0),
            knob_center.y + angle.sin() * (knob_radius - 6.0),
        );
        painter.line_segment(
            [knob_center, indicator],
            Stroke::new(3.0, self.palette.knob_indicator),
        );
        painter.circle_filled(knob_center, 3.0, self.palette.knob_indicator);

        let label_pos = egui::pos2(rect.center().x, rect.bottom() - 6.0);
        painter.text(
            label_pos,
            Align2::CENTER_BOTTOM,
            self.label,
            FontId::proportional(12.0),
            self.palette.knob_label,
        );

        response
    }
}

struct Clip {
    name: String,
    start_beat: f32,
    length_beats: f32,
    color: Color32,
    notes: Vec<Note>,
    launch_state: ClipLaunchState,
}

impl Clip {
    fn new(
        name: impl Into<String>,
        start_beat: f32,
        length_beats: f32,
        color: Color32,
        notes: Vec<Note>,
    ) -> Self {
        Self {
            name: name.into(),
            start_beat,
            length_beats,
            color,
            notes,
            launch_state: ClipLaunchState::Stopped,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipLaunchState {
    Stopped,
    Playing,
}

impl ClipLaunchState {
    fn is_playing(self) -> bool {
        matches!(self, Self::Playing)
    }

    fn button_label(self) -> &'static str {
        match self {
            ClipLaunchState::Stopped => "Launch",
            ClipLaunchState::Playing => "Stop",
        }
    }

    fn status_label(self) -> &'static str {
        match self {
            ClipLaunchState::Stopped => "Stopped",
            ClipLaunchState::Playing => "Playing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutomationPoint {
    beat: f32,
    value: f32,
}

impl AutomationPoint {
    fn new(beat: f32, value: f32) -> Self {
        Self { beat, value }
    }
}

#[derive(Debug, Clone)]
struct AutomationLane {
    parameter: String,
    points: Vec<AutomationPoint>,
    color: Color32,
    visible: bool,
}

impl AutomationLane {
    fn new(parameter: impl Into<String>, color: Color32, mut points: Vec<AutomationPoint>) -> Self {
        if points.is_empty() {
            points.push(AutomationPoint::new(0.0, 0.5));
        }
        Self {
            parameter: parameter.into(),
            points,
            color,
            visible: true,
        }
    }

    fn parameter(&self) -> &str {
        &self.parameter
    }

    fn color(&self) -> Color32 {
        self.color
    }

    fn add_point(&mut self, point: AutomationPoint) {
        self.points.push(point);
        self.points.sort_by(|a, b| {
            a.beat
                .partial_cmp(&b.beat)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Note {
    start_beats: f32,
    length_beats: f32,
    pitch: u8,
}

impl Note {
    fn new(start_beats: f32, length_beats: f32, pitch: u8) -> Self {
        Self {
            start_beats,
            length_beats,
            pitch,
        }
    }
}

#[derive(Debug)]
struct PlaylistViewState {
    pixels_per_beat: f32,
    track_height: f32,
    automation_lane_height: f32,
}

impl Default for PlaylistViewState {
    fn default() -> Self {
        Self {
            pixels_per_beat: 80.0,
            track_height: 54.0,
            automation_lane_height: 42.0,
        }
    }
}

#[derive(Debug)]
struct PianoRollState {
    pixels_per_beat: f32,
    key_height: f32,
    key_range: std::ops::RangeInclusive<u8>,
}

impl PianoRollState {
    fn key_range_len(&self) -> usize {
        (*self.key_range.end() - *self.key_range.start()) as usize + 1
    }

    fn pitch_to_y(&self, rect: egui::Rect, pitch: u8) -> f32 {
        let pitch = pitch.clamp(*self.key_range.start(), *self.key_range.end());
        let index = (pitch - *self.key_range.start()) as f32;
        rect.bottom() - index * self.key_height
    }

    fn y_to_pitch(&self, rect: egui::Rect, y: f32) -> Option<u8> {
        if y < rect.top() || y > rect.bottom() {
            return None;
        }
        let index = ((rect.bottom() - y) / self.key_height).floor();
        let pitch = *self.key_range.start() as f32 + index;
        Some(pitch as u8)
    }

    fn is_pitch_visible(&self, pitch: u8) -> bool {
        self.key_range.contains(&pitch)
    }
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            pixels_per_beat: 120.0,
            key_height: 18.0,
            key_range: 36..=84,
        }
    }
}
