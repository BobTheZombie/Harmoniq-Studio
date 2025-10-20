use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use eframe::egui::{self, Color32};
use eframe::{App, CreationContext, Frame, NativeOptions};
use egui_extras::install_image_loaders;
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine,
    TransportState,
};
use harmoniq_plugins::{GainPlugin, NoisePlugin, SineSynth};
use parking_lot::Mutex;
use tracing::error;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(author, version, about = "Harmoniq Studio prototype")]
struct Cli {
    /// Run without the native UI, performing a short offline render
    #[arg(long)]
    headless: bool,

    /// Sample rate used for the audio engine
    #[arg(long, default_value_t = 48_000.0)]
    sample_rate: f32,

    /// Block size used for internal processing
    #[arg(long, default_value_t = 512)]
    block_size: usize,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .ok();

    let args = Cli::parse();

    if args.headless {
        run_headless(args.sample_rate, args.block_size)
    } else {
        run_ui(args.sample_rate, args.block_size)
    }
}

fn run_headless(sample_rate: f32, block_size: usize) -> anyhow::Result<()> {
    let config = BufferConfig::new(sample_rate, block_size, ChannelLayout::Stereo);
    let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
    let command_queue = engine.command_queue();

    let sine = engine
        .register_processor(Box::new(SineSynth::with_frequency(220.0)))
        .context("register sine")?;
    let noise = engine
        .register_processor(Box::new(NoisePlugin::default()))
        .context("register noise")?;
    let gain = engine
        .register_processor(Box::new(GainPlugin::new(0.4)))
        .context("register gain")?;

    let mut graph_builder = GraphBuilder::new();
    let sine_node = graph_builder.add_node(sine);
    graph_builder.connect_to_mixer(sine_node, 0.7)?;
    let noise_node = graph_builder.add_node(noise);
    graph_builder.connect_to_mixer(noise_node, 0.1)?;
    let gain_node = graph_builder.add_node(gain);
    graph_builder.connect_to_mixer(gain_node, 1.0)?;

    command_queue
        .try_send(EngineCommand::ReplaceGraph(graph_builder.build()))
        .map_err(|_| anyhow!("command queue full while replacing graph"))?;
    command_queue
        .try_send(EngineCommand::SetTransport(TransportState::Playing))
        .map_err(|_| anyhow!("command queue full while updating transport"))?;

    let mut buffer = harmoniq_engine::AudioBuffer::from_config(config.clone());
    for _ in 0..10 {
        engine.process_block(&mut buffer)?;
        std::thread::sleep(Duration::from_millis(10));
    }

    println!(
        "Rendered {} frames across {} channels",
        buffer.len(),
        config.layout.channels()
    );

    Ok(())
}

fn run_ui(sample_rate: f32, block_size: usize) -> anyhow::Result<()> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    let config = BufferConfig::new(sample_rate, block_size, ChannelLayout::Stereo);
    let config_for_app = config.clone();

    eframe::run_native(
        "Harmoniq Studio",
        native_options,
        Box::new(move |cc| {
            install_image_loaders(&cc.egui_ctx);
            let config = config_for_app.clone();
            let app = match HarmoniqStudioApp::new(config, cc) {
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
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl EngineRunner {
    fn start(engine: Arc<Mutex<HarmoniqEngine>>) -> Self {
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

        Self {
            _engine: engine,
            running,
            thread: Some(thread),
        }
    }
}

impl Drop for EngineRunner {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

struct HarmoniqStudioApp {
    _engine_runner: EngineRunner,
    command_queue: harmoniq_engine::EngineCommandQueue,
    tracks: Vec<Track>,
    selected_track: Option<usize>,
    selected_clip: Option<(usize, usize)>,
    tempo: f32,
    transport_state: TransportState,
    next_track_index: usize,
    next_clip_index: usize,
    next_color_index: usize,
    piano_roll: PianoRollState,
    last_error: Option<String>,
}

impl HarmoniqStudioApp {
    fn new(config: BufferConfig, cc: &CreationContext<'_>) -> anyhow::Result<Self> {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let mut engine = HarmoniqEngine::new(config.clone()).context("failed to build engine")?;
        let sine = engine
            .register_processor(Box::new(SineSynth::with_frequency(110.0)))
            .context("register sine")?;
        let noise = engine
            .register_processor(Box::new(NoisePlugin::default()))
            .context("register noise")?;
        let gain = engine
            .register_processor(Box::new(GainPlugin::new(0.6)))
            .context("register gain")?;

        let mut graph_builder = GraphBuilder::new();
        let sine_node = graph_builder.add_node(sine);
        graph_builder.connect_to_mixer(sine_node, 0.8)?;
        let noise_node = graph_builder.add_node(noise);
        graph_builder.connect_to_mixer(noise_node, 0.2)?;
        let gain_node = graph_builder.add_node(gain);
        graph_builder.connect_to_mixer(gain_node, 1.0)?;

        engine.replace_graph(graph_builder.build())?;
        engine.set_transport(TransportState::Stopped);

        let command_queue = engine.command_queue();
        let engine = Arc::new(Mutex::new(engine));
        let engine_runner = EngineRunner::start(Arc::clone(&engine));

        let mut app = Self {
            _engine_runner: engine_runner,
            command_queue,
            tracks: vec![Track::new("Lead"), Track::new("Drums"), Track::new("Bass")],
            selected_track: Some(0),
            selected_clip: Some((0, 0)),
            tempo: 120.0,
            transport_state: TransportState::Stopped,
            next_track_index: 3,
            next_clip_index: 1,
            next_color_index: 0,
            piano_roll: PianoRollState::default(),
            last_error: None,
        };

        app.initialise_demo_clips();
        Ok(app)
    }

    fn initialise_demo_clips(&mut self) {
        let lead_intro_color = self.next_color();
        let lead_hook_color = self.next_color();
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
        }

        let drum_color = self.next_color();
        if let Some(track) = self.tracks.get_mut(1) {
            track.add_clip(Clip::new(
                "Drum Loop",
                0.0,
                16.0,
                drum_color,
                vec![Note::new(0.0, 0.5, 36), Note::new(0.5, 0.5, 38)],
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
    }

    fn stop_transport(&mut self) {
        self.transport_state = TransportState::Stopped;
        self.send_command(EngineCommand::SetTransport(self.transport_state));
    }

    fn add_track(&mut self) {
        let name = format!("Track {}", self.next_track_index + 1);
        self.next_track_index += 1;
        self.tracks.push(Track::new(name));
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
        });

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (track_idx, track) in self.tracks.iter_mut().enumerate() {
                let is_selected = self.selected_track == Some(track_idx);
                let header = ui.selectable_label(is_selected, &track.name);
                if header.clicked() {
                    self.selected_track = Some(track_idx);
                }

                ui.add_space(2.0);

                for (clip_idx, clip) in track.clips.iter_mut().enumerate() {
                    let clip_selected = self.selected_clip == Some((track_idx, clip_idx));
                    let label = format!(
                        "{} — start {:.1} • len {:.1}",
                        clip.name, clip.start_beat, clip.length_beats
                    );
                    let tooltip = format!(
                        "Clip on {}\nStart: {:.1} beats\nLength: {:.1} beats",
                        track.name, clip.start_beat, clip.length_beats
                    );
                    let response = ui
                        .add_sized(
                            [ui.available_width(), 28.0],
                            egui::SelectableLabel::new(clip_selected, label),
                        )
                        .on_hover_text(tooltip);

                    if response.clicked() {
                        self.selected_clip = Some((track_idx, clip_idx));
                        self.selected_track = Some(track_idx);
                    }

                    ui.add_space(2.0);
                }

                ui.add_space(6.0);
            }
        });
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

    fn draw_mixer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mixer");
        ui.add_space(4.0);
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for track in &mut self.tracks {
                    ui.group(|ui| {
                        ui.set_min_width(120.0);
                        ui.vertical(|ui| {
                            ui.label(&track.name);
                            ui.add_space(8.0);
                            ui.add(
                                egui::Slider::new(&mut track.volume, 0.0..=1.2)
                                    .vertical()
                                    .text("Vol"),
                            );
                            ui.add(egui::Slider::new(&mut track.pan, -1.0..=1.0).text("Pan"));
                            ui.checkbox(&mut track.muted, "Mute");
                            ui.checkbox(&mut track.solo, "Solo");
                        });
                    });
                }
            });
        });
    }
}

impl App for HarmoniqStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
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
    volume: f32,
    pan: f32,
    muted: bool,
    solo: bool,
}

impl Track {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clips: Vec::new(),
            volume: 0.9,
            pan: 0.0,
            muted: false,
            solo: false,
        }
    }

    fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
    }

    fn next_clip_start(&self) -> f32 {
        self.clips
            .iter()
            .map(|clip| clip.start_beat + clip.length_beats)
            .fold(0.0, f32::max)
    }
}

struct Clip {
    name: String,
    start_beat: f32,
    length_beats: f32,
    color: Color32,
    notes: Vec<Note>,
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
        }
    }
}

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
