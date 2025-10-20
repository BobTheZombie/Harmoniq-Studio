use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke};
use eframe::{App, CreationContext, NativeOptions};
use egui_extras::install_image_loaders;
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine,
    TransportState,
};
use harmoniq_plugins::{GainPlugin, NoisePlugin, SineSynth};
use hound::{SampleFormat, WavSpec, WavWriter};
use mp3lame_encoder::{
    self, Builder as Mp3Builder, FlushNoGap, InterleavedPcm, MonoPcm, Quality as Mp3Quality,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::error;
use tracing_subscriber::EnvFilter;

mod audio;
mod midi;

use audio::{
    available_backends, describe_layout, AudioBackend, AudioRuntimeOptions, RealtimeAudio,
};
use midi::list_midi_inputs;
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
    let mut encoder_config = flacenc::config::Encoder::default()
        .into_verified()
        .context("invalid FLAC encoder configuration")?;
    // Ensure the encoder uses a reasonable block size for the project configuration.
    if encoder_config.block_size == 0 {
        encoder_config.block_size = 4096;
    }
    let source =
        flacenc::source::MemSource::from_samples(&pcm, channels, 16, sample_rate.round() as usize);
    let stream =
        flacenc::encode_with_fixed_block_size(&encoder_config, source, encoder_config.block_size)
            .context("failed to encode FLAC stream")?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink);
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
        .set_quality(Mp3Quality::High)
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

    if runtime.is_enabled() {
        let command_queue = engine.command_queue();
        let engine = Arc::new(Mutex::new(engine));
        let stream =
            RealtimeAudio::start(Arc::clone(&engine), command_queue, config.clone(), runtime)?;

        println!(
            "Streaming realtime audio via {} on '{}' ({} layout) – press Ctrl+C to stop.",
            stream.backend(),
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
    } else {
        let mut buffer = harmoniq_engine::AudioBuffer::from_config(config.clone());
        for _ in 0..10 {
            engine.process_block(&mut buffer)?;
            std::thread::sleep(Duration::from_millis(10));
        }

        println!(
            "Rendered {} frames across {} channels at {:.1} BPM",
            buffer.len(),
            config.layout.channels(),
            args.tempo
        );
    }

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
    _engine: Arc<Mutex<HarmoniqEngine>>,
    running: Option<Arc<AtomicBool>>,
    thread: Option<std::thread::JoinHandle<()>>,
    _realtime: Option<RealtimeAudio>,
}

impl EngineRunner {
    fn start(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        command_queue: harmoniq_engine::EngineCommandQueue,
        runtime: AudioRuntimeOptions,
    ) -> anyhow::Result<Self> {
        if runtime.is_enabled() {
            let realtime =
                RealtimeAudio::start(Arc::clone(&engine), command_queue, config, runtime)?;
            Ok(Self {
                _engine: engine,
                running: None,
                thread: None,
                _realtime: Some(realtime),
            })
        } else {
            let running = Arc::new(AtomicBool::new(true));
            let thread_running = Arc::clone(&running);
            let engine_clone = Arc::clone(&engine);

            let thread = std::thread::spawn(move || {
                let mut buffer = {
                    let engine = engine_clone.lock();
                    AudioBuffer::from_config(engine.config().clone())
                };

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

            Ok(Self {
                _engine: engine,
                running: Some(running),
                thread: Some(thread),
                _realtime: None,
            })
        }
    }
}

impl Drop for EngineRunner {
    fn drop(&mut self) {
        if let Some(running) = &self.running {
            running.store(false, Ordering::SeqCst);
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

struct HarmoniqStudioApp {
    _engine_runner: EngineRunner,
    command_queue: harmoniq_engine::EngineCommandQueue,
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
    piano_roll: PianoRollState,
    last_error: Option<String>,
    status_message: Option<String>,
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
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

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

        let tracks: Vec<Track> = (0..48).map(|index| Track::with_index(index + 1)).collect();
        let track_count = tracks.len();
        let master_track = MasterChannel::default();

        let mut app = Self {
            _engine_runner: engine_runner,
            command_queue,
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
            piano_roll: PianoRollState::default(),
            last_error: None,
            status_message: None,
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
        ui.horizontal(|ui| {
            if ui
                .button(match self.transport_state {
                    TransportState::Playing => "Pause",
                    _ => "Play",
                })
                .clicked()
            {
                self.toggle_transport();
            }

            if ui.button("Stop").clicked() {
                self.stop_transport();
            }

            ui.separator();
            ui.label("Tempo");
            let tempo_response = ui.add(
                egui::DragValue::new(&mut self.tempo)
                    .clamp_range(40.0..=220.0)
                    .speed(1.0)
                    .suffix(" BPM"),
            );
            if tempo_response.changed() {
                self.send_command(EngineCommand::SetTempo(self.tempo));
            }

            ui.separator();
            ui.label("Project");
            ui.add(
                egui::TextEdit::singleline(&mut self.project_path)
                    .desired_width(160.0)
                    .hint_text("my_project.hst"),
            );
            if ui.button("Open").clicked() {
                self.open_project();
            }
            if ui.button("Save").clicked() {
                self.save_project();
            }

            ui.separator();
            ui.label("Bounce to");
            ui.add(egui::TextEdit::singleline(&mut self.bounce_path).desired_width(160.0));
            ui.label("Length");
            ui.add(
                egui::DragValue::new(&mut self.bounce_length_beats)
                    .clamp_range(1.0..=256.0)
                    .speed(1.0)
                    .suffix(" beats"),
            );
            if ui.button("Offline Bounce").clicked() {
                self.bounce_project();
            }

            ui.separator();
            ui.label("Time signature 4/4");
        });
    }

    fn draw_playlist(&mut self, ui: &mut egui::Ui) {
        ui.heading("Playlist");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Add Track").clicked() {
                self.add_track();
            }
            if ui.button("Add Clip").clicked() {
                self.add_clip_to_selected_track();
            }
            if ui.button("Add Automation Lane").clicked() {
                self.add_automation_lane_to_selected_track();
            }
        });

        ui.separator();

        enum ClipAction {
            Launch(usize, usize),
            Stop(usize, usize),
        }

        let mut pending_clip_action: Option<ClipAction> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (track_idx, track) in self.tracks.iter_mut().enumerate() {
                let is_selected = self.selected_track == Some(track_idx);
                let header = ui.selectable_label(is_selected, &track.name);
                if header.clicked() {
                    self.selected_track = Some(track_idx);
                }

                ui.add_space(2.0);

                for (clip_idx, clip) in track.clips.iter().enumerate() {
                    let clip_selected = self.selected_clip == Some((track_idx, clip_idx));
                    let label = format!(
                        "{} — start {:.1} • len {:.1}",
                        clip.name, clip.start_beat, clip.length_beats
                    );
                    let tooltip = format!(
                        "Clip on {}\nStart: {:.1} beats\nLength: {:.1} beats",
                        track.name, clip.start_beat, clip.length_beats
                    );
                    let fill = if clip_selected {
                        clip.color.gamma_multiply(0.45)
                    } else {
                        clip.color.gamma_multiply(0.25)
                    };

                    let inner = egui::Frame::group(ui.style()).fill(fill).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let launch_label = clip.launch_state.button_label();
                            let launch_button = ui.button(launch_label);
                            if launch_button.clicked() {
                                pending_clip_action = Some(if clip.launch_state.is_playing() {
                                    ClipAction::Stop(track_idx, clip_idx)
                                } else {
                                    ClipAction::Launch(track_idx, clip_idx)
                                });
                            }

                            let response = ui.add_sized(
                                [ui.available_width(), 24.0],
                                egui::SelectableLabel::new(clip_selected, label.clone()),
                            );
                            if response.clicked() {
                                self.selected_clip = Some((track_idx, clip_idx));
                                self.selected_track = Some(track_idx);
                            }
                            response.on_hover_text(tooltip.clone());
                        });
                        ui.label(format!("State: {}", clip.launch_state.status_label()));
                    });
                    inner.response.on_hover_text(tooltip);

                    ui.add_space(2.0);
                }

                ui.add_space(6.0);

                for lane in track.automation_lanes.iter_mut() {
                    let header_text = format!("Automation: {}", lane.parameter());
                    ui.collapsing(header_text, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(lane.color(), lane.parameter());
                            ui.checkbox(&mut lane.visible, "Visible");
                            if ui.button("Add Point").clicked() {
                                let template_point = lane
                                    .points
                                    .last()
                                    .cloned()
                                    .unwrap_or_else(|| AutomationPoint::new(0.0, 0.5));
                                lane.add_point(AutomationPoint::new(
                                    template_point.beat + 1.0,
                                    template_point.value,
                                ));
                            }
                        });

                        let mut remove_point: Option<usize> = None;
                        let can_remove_points = lane.points.len() > 1;
                        for (point_idx, point) in lane.points.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("Point {}", point_idx + 1));
                                ui.add(
                                    egui::DragValue::new(&mut point.beat)
                                        .clamp_range(0.0..=256.0)
                                        .speed(0.1)
                                        .suffix(" beat"),
                                );
                                ui.add(
                                    egui::Slider::new(&mut point.value, 0.0..=1.0).text("Value"),
                                );
                                if can_remove_points && ui.button("Remove").clicked() {
                                    remove_point = Some(point_idx);
                                }
                            });
                        }

                        if let Some(point_idx) = remove_point {
                            lane.points.remove(point_idx);
                        }
                    });

                    ui.add_space(6.0);
                }
            }
        });

        if let Some(action) = pending_clip_action {
            match action {
                ClipAction::Launch(track_idx, clip_idx) => self.launch_clip(track_idx, clip_idx),
                ClipAction::Stop(track_idx, clip_idx) => self.stop_clip(track_idx, clip_idx),
            }
        }
    }

    fn draw_piano_roll(&mut self, ui: &mut egui::Ui) {
        ui.heading("Piano Roll");
        ui.add_space(4.0);

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
                    painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

                    for i in 0..=num_keys {
                        let y = rect.bottom() - key_height * i as f32;
                        painter.line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(1.0, Color32::from_rgb(45, 45, 45)),
                        );
                    }

                    let total_beats = clip.length_beats.max(1.0).ceil() as usize;
                    for beat in 0..=total_beats * 4 {
                        let x = rect.left() + beat as f32 * pixels_per_beat / 4.0;
                        let color = if beat % 4 == 0 {
                            Color32::from_rgb(70, 70, 70)
                        } else {
                            Color32::from_rgb(50, 50, 50)
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
                            egui::Stroke::new(1.0, Color32::from_rgb(20, 20, 20)),
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

    fn draw_meter(ui: &mut egui::Ui, meter: &TrackMeter) {
        let desired_size = egui::vec2(32.0, 120.0);
        let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 28, 28));
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_rgb(60, 60, 60)));

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

        painter.rect_filled(left_rect, 2.0, Color32::from_rgb(0x2d, 0xb6, 0xff));
        painter.rect_filled(right_rect, 2.0, Color32::from_rgb(0xff, 0x82, 0xaa));

        let rms_height = meter.rms_level().clamp(0.0, 1.0) * max_height;
        let rms_y = rect.bottom() - gutter - rms_height;
        painter.line_segment(
            [
                egui::pos2(rect.left() + gutter, rms_y),
                egui::pos2(rect.right() - gutter, rms_y),
            ],
            Stroke::new(1.0, Color32::from_rgb(90, 90, 90)),
        );
    }

    fn draw_effects_ui(effects: &mut Vec<MixerEffect>, ui: &mut egui::Ui) {
        if effects.is_empty() {
            ui.label(RichText::new("No effects loaded").italics().weak());
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
                        .color(Color32::from_gray(170)),
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
    ) -> bool {
        let fill = if track.muted {
            Color32::from_rgb(70, 40, 40)
        } else if track.solo {
            Color32::from_rgb(40, 70, 55)
        } else if is_selected {
            Color32::from_rgb(45, 55, 75)
        } else {
            Color32::from_rgb(35, 35, 35)
        };
        let mut frame = egui::Frame::group(ui.style());
        frame.fill = fill;
        frame.stroke = Stroke::new(1.0, Color32::from_rgb(60, 60, 60));
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
                Self::draw_meter(ui, &track.meter);
                ui.add_space(6.0);
                ui.add(Knob::new(&mut track.volume, 0.0, 1.5, 0.9, "Vol"));
                ui.add(Knob::new(&mut track.pan, -1.0, 1.0, 0.0, "Pan"));
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut track.muted, "Mute");
                    ui.toggle_value(&mut track.solo, "Solo");
                });
                ui.label(
                    RichText::new(format!("{:.1} dB", track.meter.level_db()))
                        .small()
                        .color(Color32::from_gray(180)),
                );
                ui.separator();
                ui.label(RichText::new("Effects").strong().small());
                Self::draw_effects_ui(&mut track.effects, ui);
            });
        });
        ui.add_space(8.0);
        clicked
    }

    fn draw_master_strip(ui: &mut egui::Ui, master: &mut MasterChannel) {
        let mut frame = egui::Frame::group(ui.style());
        frame.fill = Color32::from_rgb(40, 45, 70);
        frame.stroke = Stroke::new(1.0, Color32::from_rgb(70, 70, 90));
        frame.show(ui, |ui| {
            ui.set_min_width(170.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(&master.name).strong());
                ui.add_space(6.0);
                Self::draw_meter(ui, &master.meter);
                ui.add_space(6.0);
                ui.add(Knob::new(&mut master.volume, 0.0, 1.5, 1.0, "Vol"));
                ui.label(
                    RichText::new(format!("{:.1} dB", master.meter.level_db()))
                        .small()
                        .color(Color32::from_gray(200)),
                );
                ui.separator();
                ui.label(RichText::new("Master Effects").strong().small());
                Self::draw_effects_ui(&mut master.effects, ui);
            });
        });
    }

    fn draw_mixer(&mut self, ui: &mut egui::Ui) {
        self.update_mixer_visuals(ui.ctx());
        ui.heading("Mixer");
        ui.add_space(4.0);
        let mut new_selection = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, track) in self.tracks.iter_mut().enumerate() {
                    if Self::draw_track_strip(ui, index, track, self.selected_track == Some(index))
                    {
                        new_selection = Some(index);
                    }
                }
                Self::draw_master_strip(ui, &mut self.master_track);
            });
        });
        if let Some(selection) = new_selection {
            self.selected_track = Some(selection);
        }
    }
}

impl App for HarmoniqStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("transport").show(ctx, |ui| self.draw_transport_toolbar(ui));

        egui::SidePanel::left("playlist")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| self.draw_playlist(ui));

        egui::TopBottomPanel::bottom("mixer")
            .resizable(true)
            .default_height(220.0)
            .show(ctx, |ui| self.draw_mixer(ui));

        egui::CentralPanel::default().show(ctx, |ui| self.draw_piano_roll(ui));

        if let Some(message) = self.status_message.clone() {
            let mut clear_message = false;
            egui::Window::new("Status")
                .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -16.0])
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(message);
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
                .show(ctx, |ui| {
                    ui.label(error);
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
}

impl<'a> Knob<'a> {
    fn new(value: &'a mut f32, min: f32, max: f32, default: f32, label: &'a str) -> Self {
        Self {
            value,
            min,
            max,
            default,
            label,
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
        painter.circle_filled(knob_center, knob_radius, Color32::from_rgb(45, 45, 45));
        painter.circle_stroke(
            knob_center,
            knob_radius,
            Stroke::new(2.0, Color32::from_rgb(90, 90, 90)),
        );

        let normalized = (value - self.min) / (self.max - self.min).max(1e-6);
        let angle = (-135.0_f32.to_radians()) + normalized * (270.0_f32.to_radians());
        let indicator = egui::pos2(
            knob_center.x + angle.cos() * (knob_radius - 6.0),
            knob_center.y + angle.sin() * (knob_radius - 6.0),
        );
        painter.line_segment(
            [knob_center, indicator],
            Stroke::new(3.0, Color32::from_rgb(220, 220, 220)),
        );
        painter.circle_filled(knob_center, 3.0, Color32::from_rgb(200, 200, 200));

        let label_pos = egui::pos2(rect.center().x, rect.bottom() - 6.0);
        painter.text(
            label_pos,
            Align2::CENTER_BOTTOM,
            self.label,
            FontId::proportional(12.0),
            Color32::from_rgb(210, 210, 210),
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
