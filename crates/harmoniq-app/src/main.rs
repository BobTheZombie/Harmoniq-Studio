use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use clap::Parser;
use eframe::egui::Margin;
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Key, KeyboardShortcut, Modifiers, PointerButton,
    ProgressBar, Rect, Rgba, RichText, Rounding, Sense, Stroke, TextStyle, TextureOptions, Vec2,
};
use eframe::{App, CreationContext, NativeOptions};
use egui_extras::{image::load_svg_bytes, install_image_loaders};
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommand, GraphBuilder, HarmoniqEngine,
    TransportState,
};
use harmoniq_plugins::{
    AudioClipMetrics, AudioEditorPlugin, GainPlugin, NoisePlugin, SineSynth, Sub808, WestCoastLead,
};
use hound::{SampleFormat, WavSpec, WavWriter};
use mp3lame_encoder::{
    self, Builder as Mp3Builder, FlushNoGap, InterleavedPcm, MonoPcm, Quality as Mp3Quality,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;

mod audio;
mod external_plugins;
mod midi;
mod typing_keyboard;

use audio::{
    available_backends, available_output_devices, describe_layout, AudioBackend,
    AudioRuntimeOptions, OutputDeviceInfo, RealtimeAudio,
};
use external_plugins::{
    external_id_from_ui, is_external_plugin_id, ExternalPluginManager, ExternalPluginSummary,
};
use harmoniq_plugin_host::{DiscoveredPlugin, PluginParam};
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
    meter_low: Color32,
    meter_mid: Color32,
    meter_high: Color32,
    meter_peak: Color32,
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
    mixer_strip_header: Color32,
    mixer_strip_header_selected: Color32,
    mixer_slot_bg: Color32,
    mixer_slot_active: Color32,
    mixer_slot_border: Color32,
    mixer_toggle_active: Color32,
    mixer_toggle_inactive: Color32,
    mixer_toggle_text: Color32,
}

impl HarmoniqPalette {
    fn new() -> Self {
        Self {
            background: Color32::from_rgb(30, 30, 30),
            panel: Color32::from_rgb(34, 34, 38),
            panel_alt: Color32::from_rgb(42, 42, 48),
            toolbar: Color32::from_rgb(36, 36, 42),
            toolbar_highlight: Color32::from_rgb(52, 52, 60),
            toolbar_outline: Color32::from_rgb(88, 88, 98),
            text_primary: Color32::from_rgb(232, 232, 240),
            text_muted: Color32::from_rgb(164, 164, 176),
            accent: Color32::from_rgb(138, 43, 226),
            accent_alt: Color32::from_rgb(166, 104, 239),
            accent_soft: Color32::from_rgb(112, 72, 196),
            success: Color32::from_rgb(82, 212, 164),
            warning: Color32::from_rgb(255, 120, 130),
            track_header: Color32::from_rgb(44, 44, 52),
            track_header_selected: Color32::from_rgb(59, 47, 79),
            track_lane_overlay: Color32::from_rgba_unmultiplied(138, 43, 226, 42),
            track_button_bg: Color32::from_rgb(48, 48, 56),
            track_button_border: Color32::from_rgb(32, 32, 38),
            automation_header: Color32::from_rgb(46, 46, 54),
            automation_header_muted: Color32::from_rgb(38, 38, 44),
            automation_lane_bg: Color32::from_rgb(34, 34, 40),
            automation_lane_hidden_bg: Color32::from_rgb(30, 30, 36),
            automation_point_border: Color32::from_rgb(54, 54, 64),
            timeline_bg: Color32::from_rgb(32, 32, 38),
            timeline_header: Color32::from_rgb(40, 40, 48),
            timeline_border: Color32::from_rgb(90, 90, 102),
            timeline_grid_primary: Color32::from_rgb(94, 80, 126),
            timeline_grid_secondary: Color32::from_rgb(58, 58, 72),
            ruler_text: Color32::from_rgb(204, 204, 214),
            clip_text_primary: Color32::from_rgb(236, 236, 246),
            clip_text_secondary: Color32::from_rgb(190, 190, 204),
            clip_border_default: Color32::from_rgb(68, 68, 80),
            clip_border_active: Color32::from_rgb(138, 43, 226),
            clip_border_playing: Color32::from_rgb(200, 140, 255),
            clip_shadow: Color32::from_rgba_unmultiplied(0, 0, 0, 120),
            piano_background: Color32::from_rgb(28, 28, 32),
            piano_grid_major: Color32::from_rgb(70, 70, 82),
            piano_grid_minor: Color32::from_rgb(50, 50, 60),
            piano_white: Color32::from_rgb(242, 242, 248),
            piano_black: Color32::from_rgb(64, 64, 74),
            meter_background: Color32::from_rgb(36, 36, 42),
            meter_border: Color32::from_rgb(64, 64, 76),
            meter_low: Color32::from_rgb(94, 210, 170),
            meter_mid: Color32::from_rgb(255, 200, 132),
            meter_high: Color32::from_rgb(255, 150, 132),
            meter_peak: Color32::from_rgb(255, 98, 118),
            meter_rms: Color32::from_rgb(194, 166, 255),
            knob_base: Color32::from_rgb(52, 52, 64),
            knob_ring: Color32::from_rgb(138, 43, 226),
            knob_indicator: Color32::from_rgb(166, 104, 239),
            knob_label: Color32::from_rgb(210, 210, 220),
            mixer_strip_bg: Color32::from_rgb(40, 40, 48),
            mixer_strip_selected: Color32::from_rgb(62, 50, 80),
            mixer_strip_solo: Color32::from_rgb(56, 88, 78),
            mixer_strip_muted: Color32::from_rgb(86, 54, 68),
            mixer_strip_border: Color32::from_rgb(90, 90, 104),
            mixer_strip_header: Color32::from_rgb(48, 48, 58),
            mixer_strip_header_selected: Color32::from_rgb(68, 56, 92),
            mixer_slot_bg: Color32::from_rgb(36, 36, 44),
            mixer_slot_active: Color32::from_rgb(52, 44, 70),
            mixer_slot_border: Color32::from_rgb(82, 82, 96),
            mixer_toggle_active: Color32::from_rgb(138, 43, 226),
            mixer_toggle_inactive: Color32::from_rgb(40, 40, 48),
            mixer_toggle_text: Color32::from_rgb(232, 232, 240),
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
        visuals.window_rounding = Rounding::same(10.0);
        visuals.widgets.noninteractive.bg_fill = palette.panel;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_muted);
        visuals.widgets.inactive.bg_fill = palette.panel_alt;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text_primary);
        visuals.widgets.inactive.rounding = Rounding::same(6.0);
        visuals.widgets.hovered.bg_fill = palette.accent_alt.gamma_multiply(0.6);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text_primary);
        visuals.widgets.hovered.rounding = Rounding::same(6.0);
        visuals.widgets.active.bg_fill = palette.accent.gamma_multiply(0.85);
        visuals.widgets.active.fg_stroke = Stroke::new(1.2, palette.text_primary);
        visuals.widgets.active.rounding = Rounding::same(6.0);
        visuals.widgets.open.bg_fill = palette.toolbar_highlight;
        visuals.selection.bg_fill = palette.accent_soft.gamma_multiply(0.85);
        visuals.selection.stroke = Stroke::new(1.0, palette.accent_alt);
        visuals.menu_rounding = Rounding::same(8.0);
        visuals.hyperlink_color = palette.accent_alt;
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.window_margin = Margin::same(10.0);
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct TimeSignature {
    numerator: u32,
    denominator: u32,
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
    fn as_tuple(&self) -> (u32, u32) {
        (self.numerator, self.denominator)
    }

    fn set_from_tuple(&mut self, value: (u32, u32)) {
        self.numerator = value.0.max(1);
        self.denominator = value.1.max(1);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
struct TransportClock {
    bars: u32,
    beats: u32,
    ticks: u32,
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
struct PluginInstanceInfo {
    id: usize,
    name: String,
    plugin_type: String,
    cpu: f32,
    latency_ms: f32,
    bypassed: bool,
    open: bool,
}

impl PluginInstanceInfo {
    fn new(id: usize, name: impl Into<String>, plugin_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            plugin_type: plugin_type.into(),
            cpu: 0.0,
            latency_ms: 0.0,
            bypassed: false,
            open: false,
        }
    }

    fn from_external(summary: &ExternalPluginSummary) -> Self {
        Self {
            id: summary.ui_id,
            name: summary.name.clone(),
            plugin_type: summary.plugin_type.clone(),
            cpu: summary.cpu,
            latency_ms: summary.latency_ms,
            bypassed: summary.bypassed,
            open: false,
        }
    }
}

#[derive(Debug)]
struct EngineContext {
    tempo: f32,
    time_signature: TimeSignature,
    transport: TransportState,
    pattern_mode: bool,
    cpu_usage: f32,
    clock: TransportClock,
    master_meter: (f32, f32),
    plugins: Vec<PluginInstanceInfo>,
    demo_plugins_seeded: bool,
}

impl EngineContext {
    fn new(tempo: f32, time_signature: TimeSignature) -> Self {
        Self {
            tempo,
            time_signature,
            transport: TransportState::Stopped,
            pattern_mode: true,
            cpu_usage: 0.0,
            clock: TransportClock::default(),
            master_meter: (0.0, 0.0),
            plugins: Vec::new(),
            demo_plugins_seeded: false,
        }
    }

    fn ensure_demo_plugins(&mut self) {
        if !self.demo_plugins_seeded {
            self.demo_plugins_seeded = true;
            self.plugins = vec![
                PluginInstanceInfo::new(0, "West Coast Lead", "Instrument"),
                PluginInstanceInfo::new(1, "Sub 808", "Instrument"),
                PluginInstanceInfo::new(2, "Harmoniq Edison", "Audio Editor"),
                PluginInstanceInfo::new(3, "Parametric EQ", "Effect"),
                PluginInstanceInfo::new(4, "Space Reverb", "Effect"),
            ];
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LayoutPersistence {
    browser_width: f32,
    channel_rack_width: f32,
    mixer_width: f32,
    piano_roll_height: f32,
    browser_visible: bool,
    plugin_rack_visible: bool,
    piano_roll_visible: bool,
}

impl Default for LayoutPersistence {
    fn default() -> Self {
        Self {
            browser_width: 240.0,
            channel_rack_width: 340.0,
            mixer_width: 360.0,
            piano_roll_height: 240.0,
            browser_visible: true,
            plugin_rack_visible: true,
            piano_roll_visible: true,
        }
    }
}

struct LayoutState {
    persistence: LayoutPersistence,
    needs_save: bool,
    last_saved: Instant,
}

impl LayoutState {
    fn load() -> Self {
        let path = Self::path();
        let persistence = if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|contents| serde_json::from_str(&contents).ok())
                .unwrap_or_default()
        } else {
            LayoutPersistence::default()
        };
        Self {
            persistence,
            needs_save: false,
            last_saved: Instant::now(),
        }
    }

    fn path() -> PathBuf {
        if let Ok(custom) = env::var("HARMONIQ_UI_LAYOUT_PATH") {
            if !custom.trim().is_empty() {
                return PathBuf::from(custom);
            }
        }
        PathBuf::from("ui_layout.json")
    }

    fn persistence(&self) -> &LayoutPersistence {
        &self.persistence
    }

    fn persistence_mut(&mut self) -> &mut LayoutPersistence {
        self.needs_save = true;
        &mut self.persistence
    }

    fn set_browser_width(&mut self, width: f32) {
        if (self.persistence.browser_width - width).abs() > 0.5 {
            self.persistence.browser_width = width.max(180.0);
            self.needs_save = true;
        }
    }

    fn set_channel_rack_width(&mut self, width: f32) {
        if (self.persistence.channel_rack_width - width).abs() > 0.5 {
            self.persistence.channel_rack_width = width.max(240.0);
            self.needs_save = true;
        }
    }

    fn set_mixer_width(&mut self, width: f32) {
        if (self.persistence.mixer_width - width).abs() > 0.5 {
            self.persistence.mixer_width = width.max(280.0);
            self.needs_save = true;
        }
    }

    fn set_piano_roll_height(&mut self, height: f32) {
        if (self.persistence.piano_roll_height - height).abs() > 0.5 {
            self.persistence.piano_roll_height = height.max(200.0);
            self.needs_save = true;
        }
    }

    fn set_browser_visible(&mut self, visible: bool) {
        if self.persistence.browser_visible != visible {
            self.persistence.browser_visible = visible;
            self.needs_save = true;
        }
    }

    fn set_plugin_rack_visible(&mut self, visible: bool) {
        if self.persistence.plugin_rack_visible != visible {
            self.persistence.plugin_rack_visible = visible;
            self.needs_save = true;
        }
    }

    fn set_piano_roll_visible(&mut self, visible: bool) {
        if self.persistence.piano_roll_visible != visible {
            self.persistence.piano_roll_visible = visible;
            self.needs_save = true;
        }
    }

    fn maybe_save(&mut self) {
        if self.needs_save && self.last_saved.elapsed() > Duration::from_secs(2) {
            if let Ok(serialised) = serde_json::to_string_pretty(&self.persistence) {
                if fs::write(Self::path(), serialised).is_ok() {
                    self.needs_save = false;
                    self.last_saved = Instant::now();
                }
            }
        }
    }

    fn flush(&mut self) {
        if let Ok(serialised) = serde_json::to_string_pretty(&self.persistence) {
            if fs::write(Self::path(), serialised).is_ok() {
                self.needs_save = false;
                self.last_saved = Instant::now();
            }
        }
    }
}

#[derive(Debug, Clone)]
struct WaveformPreview {
    path: PathBuf,
    samples: Vec<f32>,
    sample_rate: u32,
}

impl WaveformPreview {
    fn is_for(&self, other: &Path) -> bool {
        self.path == other
    }
}

#[derive(Debug)]
struct BrowserCategory {
    name: String,
    path: PathBuf,
    expanded: bool,
}

impl BrowserCategory {
    fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
            expanded: true,
        }
    }
}

#[derive(Debug)]
struct BrowserPanelState {
    categories: Vec<BrowserCategory>,
    filter: String,
    favorites: HashSet<PathBuf>,
    selected_file: Option<PathBuf>,
    waveform_cache: HashMap<PathBuf, WaveformPreview>,
    last_scan: Instant,
}

impl BrowserPanelState {
    fn new(root: &Path) -> Self {
        let samples = root.join("samples");
        let instruments = root.join("instruments");
        let presets = root.join("presets");
        let plugins = root.join("plugins");
        let projects = root.join("projects");
        let categories = vec![
            BrowserCategory::new("Samples", samples),
            BrowserCategory::new("Instruments", instruments),
            BrowserCategory::new("Presets", presets),
            BrowserCategory::new("Plugins", plugins),
            BrowserCategory::new("Projects", projects),
        ];
        Self {
            categories,
            filter: String::new(),
            favorites: HashSet::new(),
            selected_file: None,
            waveform_cache: HashMap::new(),
            last_scan: Instant::now(),
        }
    }

    fn refresh(&mut self) {
        if self.last_scan.elapsed() > Duration::from_secs(5) {
            self.waveform_cache
                .retain(|path, preview| preview.is_for(path));
            self.last_scan = Instant::now();
        }
    }

    fn toggle_favourite(&mut self, path: &Path) {
        if !self.favorites.insert(path.to_path_buf()) {
            self.favorites.remove(path);
        }
    }
}

#[derive(Debug, Default)]
struct PluginRackState {
    visible: bool,
    docked: bool,
    filter: String,
    selected_plugin: Option<usize>,
    pending_removals: Vec<usize>,
    pending_bypass: Vec<(usize, bool)>,
}

impl PluginRackState {
    fn is_match(&self, plugin: &PluginInstanceInfo) -> bool {
        if self.filter.trim().is_empty() {
            true
        } else {
            let needle = self.filter.to_ascii_lowercase();
            plugin.name.to_ascii_lowercase().contains(&needle)
                || plugin.plugin_type.to_ascii_lowercase().contains(&needle)
        }
    }

    fn queue_removal(&mut self, id: usize) {
        if !self.pending_removals.contains(&id) {
            self.pending_removals.push(id);
        }
    }

    fn queue_bypass(&mut self, id: usize, bypassed: bool) {
        if let Some(entry) = self
            .pending_bypass
            .iter_mut()
            .find(|(pending_id, _)| *pending_id == id)
        {
            entry.1 = bypassed;
        } else {
            self.pending_bypass.push((id, bypassed));
        }
    }

    fn take_pending(&mut self) -> (Vec<usize>, Vec<(usize, bool)>) {
        (
            std::mem::take(&mut self.pending_removals),
            std::mem::take(&mut self.pending_bypass),
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum CommandAction {
    TogglePianoRoll,
    ToggleBrowser,
    TogglePluginRack,
    AddTrack,
    AddInstrument,
    SaveProject,
    OpenProject,
    BounceProject,
    FocusMixer,
    FocusPlaylist,
    FocusChannelRack,
    SetPatternMode,
    SetSongMode,
}

#[derive(Debug, Clone)]
struct CommandPaletteEntry {
    id: &'static str,
    label: &'static str,
    action: CommandAction,
}

#[derive(Debug)]
struct CommandPaletteState {
    open: bool,
    query: String,
    selected: usize,
    commands: Vec<CommandPaletteEntry>,
}

impl CommandPaletteState {
    fn new() -> Self {
        let commands = vec![
            CommandPaletteEntry {
                id: "toggle_piano_roll",
                label: "Toggle Piano Roll",
                action: CommandAction::TogglePianoRoll,
            },
            CommandPaletteEntry {
                id: "toggle_browser",
                label: "Toggle Browser",
                action: CommandAction::ToggleBrowser,
            },
            CommandPaletteEntry {
                id: "toggle_plugin_rack",
                label: "Toggle Plugin Rack",
                action: CommandAction::TogglePluginRack,
            },
            CommandPaletteEntry {
                id: "add_track",
                label: "Add New Track",
                action: CommandAction::AddTrack,
            },
            CommandPaletteEntry {
                id: "add_instrument",
                label: "Add Instrument to Channel Rack",
                action: CommandAction::AddInstrument,
            },
            CommandPaletteEntry {
                id: "save_project",
                label: "Save Project",
                action: CommandAction::SaveProject,
            },
            CommandPaletteEntry {
                id: "open_project",
                label: "Open Project",
                action: CommandAction::OpenProject,
            },
            CommandPaletteEntry {
                id: "bounce_project",
                label: "Render Bounce",
                action: CommandAction::BounceProject,
            },
            CommandPaletteEntry {
                id: "focus_mixer",
                label: "Focus Mixer",
                action: CommandAction::FocusMixer,
            },
            CommandPaletteEntry {
                id: "focus_playlist",
                label: "Focus Playlist",
                action: CommandAction::FocusPlaylist,
            },
            CommandPaletteEntry {
                id: "focus_channel_rack",
                label: "Focus Channel Rack",
                action: CommandAction::FocusChannelRack,
            },
            CommandPaletteEntry {
                id: "pattern_mode",
                label: "Switch to Pattern Mode",
                action: CommandAction::SetPatternMode,
            },
            CommandPaletteEntry {
                id: "song_mode",
                label: "Switch to Song Mode",
                action: CommandAction::SetSongMode,
            },
        ];
        Self {
            open: false,
            query: String::new(),
            selected: 0,
            commands,
        }
    }

    fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let needle = self.query.trim().to_ascii_lowercase();
        self.commands
            .iter()
            .enumerate()
            .filter(|(_, command)| {
                if needle.is_empty() {
                    true
                } else {
                    command.label.to_ascii_lowercase().contains(&needle)
                }
            })
            .map(|(idx, _)| idx)
            .collect()
    }
}

#[derive(Debug, Clone)]
enum DragPayload {
    Sample(PathBuf),
    PatternClip {
        track_index: usize,
        clip_index: usize,
    },
    Plugin(usize),
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

fn quantize_to_step(value: f32, step: f32) -> f32 {
    if step <= f32::EPSILON {
        value
    } else {
        (value / step).round() * step
    }
}

fn calculate_sample_length_beats(path: &Path, tempo: f32) -> anyhow::Result<f32> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("missing file extension"))?;

    if !extension.eq_ignore_ascii_case("wav") {
        anyhow::bail!("unsupported audio format: {extension}");
    }

    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as f32;
    let frames = reader.duration() as f32 / channels;
    if spec.sample_rate == 0 {
        anyhow::bail!("sample rate of zero in {extension} file");
    }
    let seconds = frames / spec.sample_rate as f32;
    let beats = seconds * tempo.max(1.0) / 60.0;
    Ok(beats.max(0.25))
}

fn insert_sample_clip_on_track(
    tracks: &mut [Track],
    track_idx: usize,
    drop_beat: f32,
    path: PathBuf,
    tempo: f32,
    color: Color32,
) -> Result<(usize, Option<String>), String> {
    if track_idx >= tracks.len() {
        return Err(format!("Track index {track_idx} is out of range"));
    }

    let mut warning = None;
    let length_beats = match calculate_sample_length_beats(&path, tempo) {
        Ok(length) => length,
        Err(err) => {
            warning = Some(err.to_string());
            4.0
        }
    };

    let start = quantize_to_step(drop_beat.max(0.0), 0.25);
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "Sample".to_string());

    let clip = Clip::from_sample(name, start, length_beats.max(0.25), color, path);
    let index = tracks[track_idx].insert_clip_sorted(clip);
    Ok((index, warning))
}

fn generate_waveform_preview(path: &Path) -> anyhow::Result<WaveformPreview> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("missing file extension"))?;
    if !extension.eq_ignore_ascii_case("wav") {
        anyhow::bail!("unsupported audio format: {extension}");
    }
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let max_samples = (spec.sample_rate as usize).min(12_000);
    let mut samples = Vec::with_capacity(max_samples);
    for sample in reader.samples::<i16>().take(max_samples) {
        samples.push(sample? as f32 / i16::MAX as f32);
    }
    Ok(WaveformPreview {
        path: path.to_path_buf(),
        samples,
        sample_rate: spec.sample_rate,
    })
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
                    running_clone.store(false, AtomicOrdering::SeqCst);
                })?;

                while running.load(AtomicOrdering::SeqCst) {
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
            while thread_running.load(AtomicOrdering::SeqCst) {
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
        self.running.store(false, AtomicOrdering::SeqCst);
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
            .add(Knob::new(&mut value, min, max, default, label, palette).with_diameter(64.0))
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
            .resizable(true)
            .min_size(Vec2::new(420.0, 360.0))
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

struct Sub808EditorState {
    plugin: Sub808,
    show: bool,
    last_error: Option<String>,
}

impl Sub808EditorState {
    fn new(sample_rate: f32) -> Self {
        let mut plugin = Sub808::default();
        plugin.set_sample_rate(sample_rate);
        Self {
            plugin,
            show: false,
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
        egui::Window::new("808 Sub Bass")
            .open(&mut open)
            .resizable(true)
            .min_size(Vec2::new(420.0, 340.0))
            .default_width(520.0)
            .show(ctx, |ui| self.draw_contents(ui, palette));
        self.show = open;
    }

    fn draw_contents(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette) {
        ui.heading(RichText::new("808 Sub Bass").color(palette.text_primary));
        ui.label(
            RichText::new(
                "Classic TR-808 inspired sub generator with pitch drop and tone shaping.",
            )
            .color(palette.text_muted),
        );
        ui.add_space(12.0);

        ui.label(
            RichText::new("Output & Tone")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "sub808.level",
                0.0,
                1.5,
                0.9,
                "Level",
                "Overall output level of the instrument",
            );
            self.knob(
                ui,
                palette,
                "sub808.tone",
                0.0,
                1.0,
                0.55,
                "Tone",
                "Low-pass filter tilt spanning roughly 60 Hz to 8 kHz",
            );
            self.knob(
                ui,
                palette,
                "sub808.drive",
                0.0,
                1.0,
                0.25,
                "Drive",
                "Pre-filter saturation amount",
            );
            self.knob(
                ui,
                palette,
                "sub808.harmonics",
                0.0,
                1.0,
                0.15,
                "Harmonics",
                "Blend of the second harmonic for extra bite",
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
                "sub808.attack",
                0.0,
                0.08,
                0.005,
                "Attack",
                "Rise time for shaping kick drum style thumps",
            );
            self.knob(
                ui,
                palette,
                "sub808.decay",
                0.05,
                4.0,
                1.2,
                "Decay",
                "Primary tail length of the bass hit",
            );
            self.knob(
                ui,
                palette,
                "sub808.sustain",
                0.0,
                1.0,
                0.0,
                "Sustain",
                "Level held when keys are sustained",
            );
            self.knob(
                ui,
                palette,
                "sub808.release",
                0.05,
                2.5,
                0.45,
                "Release",
                "Release tail after note off",
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.label(
            RichText::new("Pitch Envelope")
                .color(palette.text_primary)
                .small()
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            self.knob(
                ui,
                palette,
                "sub808.pitch_amount",
                0.0,
                24.0,
                12.0,
                "Pitch Amt",
                "Amount of downward pitch sweep in semitones",
            );
            self.knob(
                ui,
                palette,
                "sub808.pitch_decay",
                0.05,
                1.2,
                0.35,
                "Pitch Decay",
                "Time for the pitch drop to settle",
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

struct AudioEditorState {
    plugin: AudioEditorPlugin,
    show: bool,
    last_error: Option<String>,
    status: Option<String>,
    load_path: String,
    save_path: String,
    waveform_drag_start: Option<f32>,
}

impl AudioEditorState {
    fn new(sample_rate: f32) -> Self {
        let mut plugin = AudioEditorPlugin::default();
        plugin.set_engine_sample_rate(sample_rate);
        Self {
            plugin,
            show: false,
            last_error: None,
            status: None,
            load_path: String::from("audio.wav"),
            save_path: String::from("edited.wav"),
            waveform_drag_start: None,
        }
    }

    fn open(&mut self) {
        self.show = true;
    }

    fn notify_success(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
        self.last_error = None;
    }

    fn report_error(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
        self.status = None;
    }

    fn draw(&mut self, ctx: &egui::Context, palette: &HarmoniqPalette, icons: &AppIcons) {
        if !self.show {
            return;
        }
        let mut open = self.show;
        egui::Window::new("Edison Audio Editor")
            .open(&mut open)
            .resizable(true)
            .min_size(Vec2::new(520.0, 420.0))
            .default_width(680.0)
            .show(ctx, |ui| self.draw_contents(ui, palette, icons));
        self.show = open;
    }

    fn draw_contents(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, icons: &AppIcons) {
        ui.heading(RichText::new("Edison Audio Editor").color(palette.text_primary));
        ui.label(
            RichText::new("Capture, trim, and sculpt audio clips with Harmoniq's waveform editor.")
                .color(palette.text_muted),
        );
        ui.add_space(12.0);

        if let Some(status) = &self.status {
            ui.label(RichText::new(status).color(palette.success));
        }
        if let Some(error) = &self.last_error {
            ui.label(RichText::new(error).color(palette.warning));
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Open").color(palette.text_primary));
            ui.add(egui::TextEdit::singleline(&mut self.load_path).desired_width(280.0));
            let button = egui::Button::image_and_text(
                egui::Image::from_texture(&icons.open).fit_to_exact_size(Vec2::splat(18.0)),
                "Load",
            );
            if ui.add(button).clicked() {
                if self.load_path.trim().is_empty() {
                    self.report_error("Specify a file path to load");
                } else {
                    match self.plugin.load_audio(&self.load_path) {
                        Ok(()) => {
                            self.notify_success(format!("Loaded {}", self.load_path));
                            if self.save_path.trim().is_empty() {
                                self.save_path = self.load_path.clone();
                            }
                        }
                        Err(err) => self.report_error(err.to_string()),
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new("Export").color(palette.text_primary));
            ui.add(egui::TextEdit::singleline(&mut self.save_path).desired_width(280.0));
            let button = egui::Button::image_and_text(
                egui::Image::from_texture(&icons.save).fit_to_exact_size(Vec2::splat(18.0)),
                "Save WAV",
            );
            if ui.add(button).clicked() {
                if self.save_path.trim().is_empty() {
                    self.report_error("Specify a destination path before exporting");
                } else {
                    match self.plugin.export_wav(&self.save_path) {
                        Ok(()) => self.notify_success(format!("Exported {}", self.save_path)),
                        Err(err) => self.report_error(err.to_string()),
                    }
                }
            }
        });

        if let Some(metrics) = self.plugin.clip_metrics() {
            self.draw_metrics(ui, palette, metrics);
        } else {
            ui.label(
                RichText::new("Load an audio file to enable editing controls.")
                    .color(palette.text_muted),
            );
        }

        ui.add_space(10.0);
        self.draw_waveform(ui, palette);
        ui.add_space(12.0);

        if self.plugin.has_clip() {
            self.draw_editor_controls(ui, palette, icons);
        }
    }

    fn draw_metrics(
        &self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        metrics: AudioClipMetrics,
    ) {
        let peak_text = if metrics.peak <= f32::EPSILON {
            "-∞ dBFS".to_string()
        } else {
            format!("{:.1} dBFS", 20.0 * metrics.peak.log10())
        };
        ui.label(
            RichText::new(format!(
                "{:.1} kHz • {} channels • {:.2} s ({} samples) • peak {}",
                metrics.sample_rate / 1000.0,
                metrics.channels,
                metrics.length_seconds,
                metrics.length_samples,
                peak_text
            ))
            .color(palette.text_muted),
        );
        if let Some(path) = self.plugin.source_path() {
            ui.label(
                RichText::new(format!("Source: {}", path.display())).color(palette.text_muted),
            );
        }
    }

    fn draw_editor_controls(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        icons: &AppIcons,
    ) {
        if let Some((start, end)) = self.plugin.selection_seconds() {
            let length = (end - start).max(0.0);
            ui.label(
                RichText::new(format!(
                    "Selection: {:.3} – {:.3} s (Δ {:.3} s)",
                    start, end, length
                ))
                .color(palette.text_primary),
            );
        } else {
            ui.label(RichText::new("Selection: none").color(palette.text_muted));
        }
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let playing = self.plugin.is_playing();
            let play_icon = if playing { &icons.pause } else { &icons.play };
            let play_label = if playing { "Pause" } else { "Play" };
            let button = egui::Button::image_and_text(
                egui::Image::from_texture(play_icon).fit_to_exact_size(Vec2::splat(18.0)),
                play_label,
            );
            if ui.add(button).clicked() {
                if playing {
                    self.plugin.stop_playback();
                } else {
                    let use_selection = self
                        .plugin
                        .selection_samples()
                        .map(|(s, e)| e > s)
                        .unwrap_or(false);
                    self.plugin.start_playback(use_selection);
                }
            }

            let stop_button = egui::Button::image_and_text(
                egui::Image::from_texture(&icons.stop).fit_to_exact_size(Vec2::splat(18.0)),
                "Stop",
            );
            if ui.add(stop_button).clicked() {
                self.plugin.stop_playback();
            }

            if ui.button("Play selection").clicked() {
                self.plugin.start_playback(true);
            }
            if ui.button("Play from start").clicked() {
                self.plugin.start_playback(false);
            }

            let mut loop_enabled = self.plugin.loop_enabled();
            if ui.checkbox(&mut loop_enabled, "Loop").changed() {
                if let Err(err) = self.plugin.set_loop_enabled(loop_enabled) {
                    self.report_error(err);
                } else if loop_enabled {
                    self.notify_success("Loop enabled");
                } else {
                    self.notify_success("Loop disabled");
                }
            }

            let mut gain = self.plugin.output_gain();
            if ui
                .add(
                    egui::DragValue::new(&mut gain)
                        .clamp_range(0.0..=2.5)
                        .speed(0.01)
                        .prefix("Gain ")
                        .suffix("x"),
                )
                .changed()
            {
                if let Err(err) = self.plugin.set_output_gain(gain) {
                    self.report_error(err);
                }
            }
        });

        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("Playhead: {:.3} s", self.plugin.playhead_seconds()))
                .color(palette.text_muted),
        );

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Clear selection").clicked() {
                self.plugin.clear_selection();
            }
            if ui.button("Trim to selection").clicked() {
                match self.plugin.apply_trim() {
                    Ok(()) => self.notify_success("Trimmed selection"),
                    Err(err) => self.report_error(err),
                }
            }
            if ui.button("Normalize").clicked() {
                self.plugin.apply_normalize();
                self.notify_success("Normalized amplitude");
            }
            if ui.button("Reverse").clicked() {
                self.plugin.apply_reverse();
                self.notify_success("Reversed region");
            }
            if ui.button("Fade in").clicked() {
                self.plugin.apply_fade_in();
                self.notify_success("Applied fade in");
            }
            if ui.button("Fade out").clicked() {
                self.plugin.apply_fade_out();
                self.notify_success("Applied fade out");
            }
        });
    }

    fn draw_waveform(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette) {
        let width = ui.available_width();
        let desired = egui::vec2(width, 200.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, egui::Rounding::same(8.0), palette.panel_alt);
        painter.rect_stroke(
            rect,
            egui::Rounding::same(8.0),
            egui::Stroke::new(1.0, palette.toolbar_outline),
        );

        if !self.plugin.has_clip() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Load an audio file to view the waveform",
                FontId::proportional(16.0),
                palette.text_muted,
            );
        } else {
            let segments = rect.width().max(1.0) as usize;
            let overview = self.plugin.waveform_overview(segments.max(1));
            let mid_y = rect.center().y;
            let amplitude = rect.height() * 0.5;
            let stroke = egui::Stroke::new(1.0, palette.accent);
            let total = overview.len().max(1) as f32;
            for (index, (min, max)) in overview.iter().enumerate() {
                let fraction = index as f32 / total;
                let x = rect.left() + fraction * rect.width();
                let y_min = mid_y - max.clamp(-1.0, 1.0) * amplitude;
                let y_max = mid_y - min.clamp(-1.0, 1.0) * amplitude;
                painter.line_segment([egui::pos2(x, y_min), egui::pos2(x, y_max)], stroke);
            }

            if let Some((start, end)) = self.plugin.selection_samples() {
                let total_samples = self.plugin.clip_length_samples().max(1);
                let left = rect.left() + (start as f32 / total_samples as f32) * rect.width();
                let right = rect.left() + (end as f32 / total_samples as f32) * rect.width();
                let selection_rect = egui::Rect::from_min_max(
                    egui::pos2(left, rect.top()),
                    egui::pos2(right, rect.bottom()),
                );
                painter.rect_filled(
                    selection_rect,
                    egui::Rounding::ZERO,
                    palette.accent_soft.linear_multiply(0.45),
                );
            }

            let play_fraction = self.plugin.playhead_fraction();
            let play_x = rect.left() + play_fraction * rect.width();
            painter.line_segment(
                [
                    egui::pos2(play_x, rect.top()),
                    egui::pos2(play_x, rect.bottom()),
                ],
                egui::Stroke::new(1.5, palette.accent_alt),
            );
        }

        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let fraction = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.waveform_drag_start = Some(fraction);
                self.plugin.set_selection_fraction(fraction, fraction);
            }
        }
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(start) = self.waveform_drag_start {
                    let fraction = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let (a, b) = if fraction >= start {
                        (start, fraction)
                    } else {
                        (fraction, start)
                    };
                    self.plugin.set_selection_fraction(a, b);
                }
            }
        }
        if response.drag_stopped() {
            self.waveform_drag_start = None;
        }
        if response.clicked() && !response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let fraction = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.plugin.set_playhead_fraction(fraction);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct MixerConsolePluginState {
    show: bool,
    default_size: Vec2,
    min_size: Vec2,
}

impl Default for MixerConsolePluginState {
    fn default() -> Self {
        Self {
            show: true,
            default_size: Vec2::new(960.0, 420.0),
            min_size: Vec2::new(640.0, 320.0),
        }
    }
}

impl MixerConsolePluginState {
    fn open(&mut self) {
        self.show = true;
    }
}

struct AppState {
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
    sequencer: SequencerState,
    focused_instrument: Option<usize>,
    piano_roll: PianoRollState,
    piano_roll_selected_note: Option<usize>,
    piano_roll_drag: Option<PianoRollDragState>,
    last_error: Option<String>,
    status_message: Option<String>,
    audio_settings: AudioSettingsState,
    westcoast_editor: WestCoastEditorState,
    sub808_editor: Sub808EditorState,
    audio_editor: AudioEditorState,
    mixer_console: MixerConsolePluginState,
    project_path: String,
    bounce_path: String,
    bounce_length_beats: f32,
    engine_context: Arc<Mutex<EngineContext>>,
    time_signature: TimeSignature,
    pattern_mode: bool,
    loop_enabled: bool,
    transport_clock: TransportClock,
    playback_position_beats: f32,
    last_transport_tick: Instant,
    layout: LayoutState,
    browser_panel: BrowserPanelState,
    plugin_rack: PluginRackState,
    command_palette: CommandPaletteState,
    drag_payload: Option<DragPayload>,
    external_plugins: ExternalPluginManager,
}

struct HarmoniqStudioApp {
    theme: HarmoniqTheme,
    icons: AppIcons,
    state: AppState,
}

impl Deref for HarmoniqStudioApp {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for HarmoniqStudioApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
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

        let mut layout = LayoutState::load();
        let resources_root = env::current_dir()
            .map(|path| path.join("resources"))
            .unwrap_or_else(|_| PathBuf::from("resources"));
        let browser_panel = BrowserPanelState::new(&resources_root);
        let mut plugin_rack = PluginRackState::default();
        plugin_rack.visible = layout.persistence().plugin_rack_visible;

        let mut external_plugins = ExternalPluginManager::new();

        let time_signature = TimeSignature::default();
        let mut context = EngineContext::new(initial_tempo, time_signature);
        context.ensure_demo_plugins();
        let engine_context = Arc::new(Mutex::new(context));

        let state = AppState {
            engine_runner,
            command_queue,
            typing_keyboard: TypingKeyboard::default(),
            engine_config: config.clone(),
            graph_config,
            tracks,
            master_track,
            selected_track: Some(0),
            selected_clip: None,
            tempo: initial_tempo,
            transport_state: TransportState::Stopped,
            next_track_index: track_count,
            next_clip_index: 1,
            next_color_index: 0,
            playlist: PlaylistViewState::default(),
            sequencer: SequencerState::default(),
            focused_instrument: None,
            piano_roll: PianoRollState::default(),
            piano_roll_selected_note: None,
            piano_roll_drag: None,
            last_error: None,
            status_message: startup_status,
            audio_settings,
            westcoast_editor: WestCoastEditorState::new(config.sample_rate),
            sub808_editor: Sub808EditorState::new(config.sample_rate),
            audio_editor: AudioEditorState::new(config.sample_rate),
            mixer_console: MixerConsolePluginState::default(),
            project_path: "project.hst".into(),
            bounce_path: "bounce.wav".into(),
            bounce_length_beats: 16.0,
            engine_context,
            time_signature,
            pattern_mode: true,
            loop_enabled: false,
            transport_clock: TransportClock::default(),
            playback_position_beats: 0.0,
            last_transport_tick: Instant::now(),
            layout,
            browser_panel,
            plugin_rack,
            command_palette: CommandPaletteState::new(),
            drag_payload: None,
            external_plugins,
        };

        let mut app = Self {
            theme,
            icons,
            state,
        };

        app.initialise_demo_clips();
        app.initialise_demo_sequencer();
        if app
            .tracks
            .get(0)
            .and_then(|track| track.clips.get(0))
            .is_some()
        {
            app.focus_clip(0, 0);
        }
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

    fn initialise_demo_sequencer(&mut self) {
        if !self.sequencer.instruments.is_empty() || self.tracks.is_empty() {
            return;
        }

        let assignments = [
            (InstrumentPlugin::WestCoastLead, 0usize, Some(0usize)),
            (InstrumentPlugin::NoiseDrums, 1usize, Some(0usize)),
            (InstrumentPlugin::Sub808, 2usize, Some(0usize)),
        ];

        for (plugin, track_idx, clip_idx) in assignments {
            if track_idx >= self.tracks.len() {
                continue;
            }
            let mut instrument = SequencerInstrument::new(
                self.sequencer.allocate_id(),
                self.tracks
                    .get(track_idx)
                    .map(|track| track.name.clone())
                    .unwrap_or_else(|| plugin.display_name().to_string()),
                plugin,
                track_idx,
                plugin.default_pattern(),
            );
            if let Some(clip_idx) = clip_idx {
                if self
                    .tracks
                    .get(track_idx)
                    .and_then(|track| track.clips.get(clip_idx))
                    .is_some()
                {
                    instrument.clip = Some(ClipReference {
                        track_index: track_idx,
                        clip_index: clip_idx,
                    });
                }
            }
            let index = self.sequencer.push_instrument(instrument);
            self.ensure_instrument_clip(index);
        }
    }

    fn refresh_sequencer_clips(&mut self) {
        if self.tracks.is_empty() {
            for instrument in &mut self.sequencer.instruments {
                instrument.clip = None;
            }
            return;
        }

        let last_track = self.tracks.len() - 1;
        for index in 0..self.sequencer.instruments.len() {
            if self.sequencer.instruments[index].mixer_track > last_track {
                self.sequencer.instruments[index].mixer_track = last_track;
                self.sequencer.instruments[index].clip = None;
            }
            let _ = self.ensure_instrument_clip(index);
        }
    }

    fn ensure_instrument_clip(&mut self, instrument_idx: usize) -> Option<ClipReference> {
        if self.tracks.is_empty() {
            if let Some(instrument) = self.sequencer.instruments.get_mut(instrument_idx) {
                instrument.clip = None;
            }
            return None;
        }

        let tracks_len = self.tracks.len();
        let (track_idx, instrument_name, pattern_length, existing_reference) = {
            let instrument = self.sequencer.instruments.get_mut(instrument_idx)?;

            if instrument.mixer_track >= tracks_len {
                instrument.mixer_track = tracks_len - 1;
                instrument.clip = None;
            }

            (
                instrument.mixer_track,
                instrument.name.clone(),
                instrument.pattern.total_length(),
                instrument.clip,
            )
        };

        if let Some(reference) = existing_reference {
            let clip_valid = self
                .tracks
                .get(reference.track_index)
                .is_some_and(|track| reference.clip_index < track.clips.len());
            if clip_valid {
                return Some(reference);
            }
            if let Some(instrument) = self.sequencer.instruments.get_mut(instrument_idx) {
                instrument.clip = None;
            }
        }

        let existing_index = self.tracks.get(track_idx).and_then(|track| {
            track
                .clips
                .iter()
                .enumerate()
                .find(|(_, clip)| clip.name == instrument_name)
                .map(|(idx, _)| idx)
        });

        let clip_index = if let Some(idx) = existing_index {
            idx
        } else {
            let color = self.next_color();
            let mut clip = Clip::new(
                instrument_name.clone(),
                0.0,
                pattern_length.max(4.0),
                color,
                Vec::new(),
            );
            clip.length_beats = clip.length_beats.max(pattern_length);
            let track = self.tracks.get_mut(track_idx)?;
            track.add_clip(clip);
            track.clips.len() - 1
        };

        if let Some(track) = self.tracks.get_mut(track_idx) {
            if let Some(clip) = track.clips.get_mut(clip_index) {
                clip.length_beats = clip.length_beats.max(pattern_length);
            }
        }

        let reference = ClipReference {
            track_index: track_idx,
            clip_index,
        };

        if let Some(instrument) = self.sequencer.instruments.get_mut(instrument_idx) {
            instrument.clip = Some(reference);
        }

        Some(reference)
    }

    fn add_sequencer_instrument(&mut self, plugin: InstrumentPlugin) {
        if self.tracks.is_empty() {
            self.last_error = Some("Add a track before inserting instruments".into());
            self.status_message = None;
            return;
        }

        let track_idx = self
            .selected_track
            .unwrap_or(0)
            .min(self.tracks.len().saturating_sub(1));
        let name = self.sequencer.next_name_for(plugin);
        let instrument = SequencerInstrument::new(
            self.sequencer.allocate_id(),
            name.clone(),
            plugin,
            track_idx,
            plugin.default_pattern(),
        );
        let index = self.sequencer.push_instrument(instrument);
        self.focused_instrument = Some(index);
        let reference = self.ensure_instrument_clip(index);
        if let Some(track) = self.tracks.get(track_idx) {
            self.status_message = Some(format!("Added {name} to {}", track.name));
        }
        if reference.is_none() {
            self.last_error = Some("Unable to create clip for instrument".into());
        } else {
            self.last_error = None;
        }
    }

    fn assign_instrument_to_track(&mut self, instrument_idx: usize, track_idx: usize) {
        if track_idx >= self.tracks.len() {
            return;
        }
        if let Some(instrument) = self.sequencer.instruments.get_mut(instrument_idx) {
            instrument.mixer_track = track_idx;
            instrument.clip = None;
            let name = instrument.name.clone();
            let routed_to = self.tracks.get(track_idx).map(|track| track.name.clone());
            let _ = self.ensure_instrument_clip(instrument_idx);
            if let Some(track_name) = routed_to {
                self.status_message = Some(format!("Routed {name} to {track_name}"));
            }
        }
    }

    fn remove_sequencer_instrument(&mut self, instrument_idx: usize) {
        if instrument_idx >= self.sequencer.instruments.len() {
            return;
        }
        let removed = self.sequencer.instruments.remove(instrument_idx);
        if let Some(reference) = removed.clip {
            if self.selected_clip == Some((reference.track_index, reference.clip_index)) {
                self.set_selected_clip(None);
            }
        }
        if let Some(focused) = self.focused_instrument {
            if focused == instrument_idx {
                self.focused_instrument = None;
            } else if focused > instrument_idx {
                self.focused_instrument = Some(focused - 1);
            }
        }
        self.status_message = Some(format!("Removed {}", removed.name));
    }

    fn instrument_step_active(
        &self,
        reference: ClipReference,
        pattern: &SequencerPattern,
        step: usize,
    ) -> bool {
        self.tracks
            .get(reference.track_index)
            .and_then(|track| track.clips.get(reference.clip_index))
            .map(|clip| {
                let start = pattern.step_start(step);
                let tolerance = pattern.tolerance();
                clip.notes.iter().any(|note| {
                    note.pitch == pattern.base_pitch
                        && (note.start_beats - start).abs() <= tolerance
                })
            })
            .unwrap_or(false)
    }

    fn toggle_instrument_step(&mut self, instrument_idx: usize, step: usize, enable: bool) {
        if self.ensure_instrument_clip(instrument_idx).is_none() {
            return;
        }

        let pattern = self.sequencer.instruments[instrument_idx].pattern;
        let Some(reference) = self.sequencer.instruments[instrument_idx].clip else {
            return;
        };

        if let Some(track) = self.tracks.get_mut(reference.track_index) {
            if let Some(clip) = track.clips.get_mut(reference.clip_index) {
                let start = pattern.step_start(step);
                let end = start + pattern.step_length;
                let tolerance = pattern.tolerance();
                if enable {
                    let exists = clip.notes.iter().any(|note| {
                        note.pitch == pattern.base_pitch
                            && (note.start_beats - start).abs() <= tolerance
                    });
                    if !exists {
                        clip.notes
                            .push(Note::new(start, pattern.step_length, pattern.base_pitch));
                        clip.notes.sort_by(|a, b| {
                            a.start_beats
                                .partial_cmp(&b.start_beats)
                                .unwrap_or(Ordering::Equal)
                        });
                    }
                    clip.length_beats = clip.length_beats.max(end);
                } else {
                    clip.notes.retain(|note| {
                        if note.pitch != pattern.base_pitch {
                            return true;
                        }
                        (note.start_beats - start).abs() > tolerance
                    });
                }
            }
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

    fn set_selected_clip(&mut self, selection: Option<(usize, usize)>) {
        self.selected_clip = selection;
        self.piano_roll_selected_note = None;
        self.piano_roll_drag = None;
    }

    fn focus_clip(&mut self, track_idx: usize, clip_idx: usize) {
        self.selected_track = Some(track_idx);
        self.set_selected_clip(Some((track_idx, clip_idx)));
    }

    fn ensure_clip_for_track(&mut self, track_idx: usize) -> Option<usize> {
        let needs_clip = match self.tracks.get(track_idx) {
            Some(track) => track.clips.is_empty(),
            None => return None,
        };

        if needs_clip {
            let clip_number = self.next_clip_index + 1;
            let color = self.next_color();
            self.next_clip_index += 1;
            let clip_name = format!("Clip {clip_number}");
            let track = self.tracks.get_mut(track_idx)?;
            track.add_clip(Clip::new(clip_name, 0.0, 4.0, color, Vec::new()));
            Some(track.clips.len() - 1)
        } else {
            Some(0)
        }
    }

    fn focus_piano_roll_on_track(&mut self, track_idx: usize) {
        if track_idx >= self.tracks.len() {
            return;
        }
        let existing_selection = self
            .selected_clip
            .filter(|(selected_track, _)| *selected_track == track_idx)
            .map(|(_, clip_idx)| clip_idx);

        let clip_idx = existing_selection.or_else(|| self.ensure_clip_for_track(track_idx));
        if let Some(clip_idx) = clip_idx {
            self.focus_clip(track_idx, clip_idx);
        } else {
            self.selected_track = Some(track_idx);
            self.set_selected_clip(None);
        }
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
            TransportState::Recording => TransportState::Playing,
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
        } else {
            self.last_transport_tick = Instant::now();
        }
    }

    fn stop_transport(&mut self) {
        self.transport_state = TransportState::Stopped;
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.stop_all_clips();
        self.playback_position_beats = 0.0;
        self.transport_clock = TransportClock::from_beats(0.0, self.time_signature);
        self.status_message = Some("Transport stopped".into());
    }

    fn add_track(&mut self) {
        self.next_track_index += 1;
        let track_index = self.next_track_index;
        self.tracks.push(Track::with_index(track_index));
    }

    fn new_project(&mut self) {
        self.tracks = (0..8).map(|index| Track::with_index(index + 1)).collect();
        self.master_track = MasterChannel::default();
        self.next_track_index = self.tracks.len();
        self.next_clip_index = 0;
        self.next_color_index = 0;
        self.piano_roll = PianoRollState::default();
        self.piano_roll_selected_note = None;
        self.piano_roll_drag = None;
        self.sequencer = SequencerState::default();
        if self.tracks.is_empty() {
            self.selected_track = None;
            self.set_selected_clip(None);
        } else {
            self.selected_track = Some(0);
            if let Some(clip_idx) = self.ensure_clip_for_track(0) {
                self.focus_clip(0, clip_idx);
            } else {
                self.set_selected_clip(None);
            }
        }
        self.transport_state = TransportState::Stopped;
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.stop_all_clips();
        self.status_message = Some("New project created".into());
        self.last_error = None;
        self.project_path = "project.hst".into();
        self.refresh_sequencer_clips();
        self.playback_position_beats = 0.0;
        self.transport_clock = TransportClock::default();
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
            sequencer: self.sequencer.clone(),
        }
    }

    fn apply_project_document(&mut self, document: ProjectDocument) -> anyhow::Result<()> {
        self.graph_config = document.graph_config;
        self.tracks = document
            .tracks
            .into_iter()
            .map(ProjectTrack::into_track)
            .collect();
        if self.tracks.is_empty() {
            self.selected_track = None;
            self.set_selected_clip(None);
        } else if self
            .tracks
            .get(0)
            .map(|track| track.clips.is_empty())
            .unwrap_or(true)
        {
            self.selected_track = Some(0);
            self.set_selected_clip(None);
        } else {
            self.focus_clip(0, 0);
        }
        self.master_track = document.master.into_master();
        self.next_track_index = document.next_track_index.max(self.tracks.len());
        self.next_clip_index = document.next_clip_index;
        self.next_color_index = document.next_color_index;
        self.bounce_path = document.bounce_path;
        self.bounce_length_beats = document.bounce_length_beats;
        self.tempo = document.tempo;
        self.sequencer = document.sequencer;
        self.transport_state = TransportState::Stopped;
        self.stop_all_clips();
        self.send_command(EngineCommand::SetTransport(self.transport_state));
        self.send_command(EngineCommand::SetTempo(self.tempo));
        self.refresh_sequencer_clips();
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
            self.focus_clip(track_idx, clip_idx);
        }
    }

    fn insert_sample_clip(&mut self, track_idx: usize, beat: f32, path: PathBuf) {
        if self.tracks.is_empty() {
            self.last_error = Some("Create a track before dropping samples".into());
            self.status_message = None;
            return;
        }
        if track_idx >= self.tracks.len() {
            self.last_error = Some(format!("Track {track_idx} is unavailable"));
            self.status_message = None;
            return;
        }

        let file_name = path
            .file_name()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Sample")
            .to_string();
        let color = self.next_color();
        let tempo = self.tempo;
        match insert_sample_clip_on_track(&mut self.tracks, track_idx, beat, path, tempo, color) {
            Ok((clip_index, warning)) => {
                self.next_clip_index += 1;
                self.focus_clip(track_idx, clip_index);
                let track_name = self.tracks[track_idx].name.clone();
                self.status_message = Some(format!("Added '{file_name}' to {track_name}"));
                if let Some(warning) = warning {
                    self.last_error = Some(format!(
                        "Unable to analyse sample length for '{file_name}': {warning}. Using default length."
                    ));
                } else {
                    self.last_error = None;
                }
            }
            Err(err) => {
                self.last_error = Some(err);
                self.status_message = None;
            }
        }
    }

    fn insert_sample_instrument(&mut self, path: PathBuf) {
        if self.tracks.is_empty() {
            self.last_error = Some("Create a track before adding instruments".into());
            self.status_message = None;
            return;
        }

        let track_idx = self
            .selected_track
            .unwrap_or(0)
            .min(self.tracks.len().saturating_sub(1));
        let track_name = self.tracks[track_idx].name.clone();
        let instrument_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "Sampler".to_string());

        let mut instrument = SequencerInstrument::new(
            self.sequencer.allocate_id(),
            instrument_name.clone(),
            InstrumentPlugin::Sampler,
            track_idx,
            InstrumentPlugin::Sampler.default_pattern(),
        );
        instrument.sample_path = Some(path);
        let index = self.sequencer.push_instrument(instrument);
        self.focused_instrument = Some(index);
        match self.ensure_instrument_clip(index) {
            Some(_) => {
                self.status_message =
                    Some(format!("Added sampler '{instrument_name}' to {track_name}"));
                self.last_error = None;
            }
            None => {
                self.status_message = Some(format!(
                    "Sampler '{instrument_name}' added to {track_name}, clip pending"
                ));
                self.last_error = Some("Unable to create clip for new sampler".into());
            }
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

    fn draw_main_menu(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        egui::Frame::none()
            .fill(palette.toolbar)
            .stroke(Stroke::new(1.0, palette.toolbar_outline))
            .rounding(Rounding::same(18.0))
            .inner_margin(Margin::symmetric(16.0, 8.0))
            .show(ui, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New Project").clicked() {
                            self.new_project();
                            ui.close_menu();
                        }
                        if ui.button("Open…").clicked() {
                            self.open_project();
                            ui.close_menu();
                        }
                        if ui.button("Save").clicked() {
                            self.save_project();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Export Audio").clicked() {
                            self.bounce_project();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        ui.add_enabled(false, egui::Button::new("Undo"));
                        ui.add_enabled(false, egui::Button::new("Redo"));
                        ui.separator();
                        ui.add_enabled(false, egui::Button::new("Cut"));
                        ui.add_enabled(false, egui::Button::new("Copy"));
                        ui.add_enabled(false, egui::Button::new("Paste"));
                    });

                    ui.menu_button("View", |ui| {
                        if ui.button("Zoom In Piano Roll").clicked() {
                            self.piano_roll.pixels_per_beat =
                                (self.piano_roll.pixels_per_beat * 1.2).clamp(40.0, 480.0);
                            ui.close_menu();
                        }
                        if ui.button("Zoom Out Piano Roll").clicked() {
                            self.piano_roll.pixels_per_beat =
                                (self.piano_roll.pixels_per_beat / 1.2).clamp(40.0, 480.0);
                            ui.close_menu();
                        }
                        if ui.button("Reset Piano Roll View").clicked() {
                            let defaults = PianoRollState::default();
                            self.piano_roll.pixels_per_beat = defaults.pixels_per_beat;
                            self.piano_roll.key_height = defaults.key_height;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Options", |ui| {
                        if ui.button("Grid: 1/4 beat").clicked() {
                            self.piano_roll.grid_division = 0.25;
                            ui.close_menu();
                        }
                        if ui.button("Grid: 1/8 beat").clicked() {
                            self.piano_roll.grid_division = 0.125;
                            ui.close_menu();
                        }
                        if ui.button("Grid: 1/16 beat").clicked() {
                            self.piano_roll.grid_division = 0.0625;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Refresh Audio Devices").clicked() {
                            self.audio_settings.refresh_devices();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Tools", |ui| {
                        if ui.button("Add Track").clicked() {
                            self.add_track();
                            ui.close_menu();
                        }
                        if ui.button("Render Bounce").clicked() {
                            self.bounce_project();
                            ui.close_menu();
                        }
                        if ui.button("Open Piano Roll on Lead").clicked() {
                            self.focus_piano_roll_on_track(0);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Help", |ui| {
                        if ui.button("About Harmoniq Studio").clicked() {
                            self.status_message = Some(
                                "Harmoniq Studio prototype – FL-style piano roll and playlist"
                                    .into(),
                            );
                            ui.close_menu();
                        }
                        if ui.button("Project Website").clicked() {
                            self.status_message =
                                Some("Visit harmoniq.studio for roadmap and updates.".into());
                            ui.close_menu();
                        }
                    });
                });
            });
    }

    fn draw_transport_toolbar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        egui::Frame::none()
            .fill(palette.toolbar)
            .stroke(Stroke::new(1.0, palette.toolbar_outline))
            .rounding(Rounding::same(18.0))
            .inner_margin(Margin::symmetric(20.0, 14.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(16.0, 12.0);

                    let playing = matches!(self.transport_state, TransportState::Playing);
                    let recording = matches!(self.transport_state, TransportState::Recording);
                    let play_icon = if playing {
                        &self.icons.pause
                    } else {
                        &self.icons.play
                    };
                    if self
                        .gradient_icon_button(
                            ui,
                            play_icon,
                            if playing { "Pause" } else { "Play" },
                            (palette.accent, palette.accent_alt),
                            playing,
                            Vec2::new(132.0, 44.0),
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
                            Vec2::new(112.0, 44.0),
                        )
                        .clicked()
                    {
                        self.stop_transport();
                    }

                    let record_response = ui
                        .add(
                            egui::Button::new(
                                RichText::new("Rec").color(palette.text_primary).size(16.0),
                            )
                            .fill(if recording {
                                palette.warning.gamma_multiply(0.35)
                            } else {
                                palette.panel_alt
                            })
                            .min_size(Vec2::new(92.0, 44.0)),
                        )
                        .on_hover_text("Arm the transport for recording");
                    if record_response.clicked() {
                        self.transport_state = if recording {
                            TransportState::Playing
                        } else {
                            TransportState::Recording
                        };
                        self.send_command(EngineCommand::SetTransport(self.transport_state));
                    }

                    let mut loop_toggle = self.loop_enabled;
                    if ui.toggle_value(&mut loop_toggle, "Loop").clicked() {
                        self.loop_enabled = loop_toggle;
                    }

                    ui.separator();

                    ui.vertical(|ui| {
                        self.section_label(ui, &self.icons.tempo, "Tempo & Meter");
                        let tempo_response = ui.add(
                            egui::DragValue::new(&mut self.tempo)
                                .clamp_range(40.0..=220.0)
                                .speed(0.5)
                                .suffix(" BPM"),
                        );
                        if tempo_response.changed() {
                            self.send_command(EngineCommand::SetTempo(self.tempo));
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Time Sig").color(palette.text_muted));
                            let mut numerator = self.time_signature.numerator as i32;
                            let mut denominator = self.time_signature.denominator as i32;
                            if ui
                                .add(egui::DragValue::new(&mut numerator).clamp_range(1..=12))
                                .changed()
                            {
                                let denominator_value = self.time_signature.denominator;
                                self.time_signature
                                    .set_from_tuple((numerator.max(1) as u32, denominator_value));
                            }
                            ui.label("/");
                            if ui
                                .add(egui::DragValue::new(&mut denominator).clamp_range(1..=16))
                                .changed()
                            {
                                let numerator_value = self.time_signature.numerator;
                                self.time_signature
                                    .set_from_tuple((numerator_value, denominator.max(1) as u32));
                            }
                        });
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                        ui.label(RichText::new("Mode").color(palette.text_muted));
                        ui.horizontal(|ui| {
                            let pattern_selected = self.pattern_mode;
                            if ui.selectable_label(pattern_selected, "Pattern").clicked() {
                                self.pattern_mode = true;
                            }
                            if ui.selectable_label(!pattern_selected, "Song").clicked() {
                                self.pattern_mode = false;
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(self.transport_clock.format())
                                .monospace()
                                .size(20.0)
                                .color(palette.text_primary),
                        );
                        ui.label(
                            RichText::new("Bars:Beats:Ticks")
                                .color(palette.text_muted)
                                .size(11.0),
                        );
                    });

                    ui.separator();

                    let cpu_usage = self.estimate_cpu_usage();
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Audio Engine").color(palette.text_muted));
                        ui.add(
                            ProgressBar::new(cpu_usage)
                                .text(format!("CPU {:>4.1}%", cpu_usage * 100.0)),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Master").color(palette.text_muted));
                            Self::draw_meter(
                                ui,
                                &self.master_track.meter,
                                &palette,
                                Vec2::new(38.0, 80.0),
                            );
                        });
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                        ui.label(RichText::new("Panels").color(palette.text_muted));
                        let mut browser = self.layout.persistence().browser_visible;
                        if ui.checkbox(&mut browser, "Browser").clicked() {
                            self.layout.set_browser_visible(browser);
                        }
                        let mut piano_roll = self.layout.persistence().piano_roll_visible;
                        if ui.checkbox(&mut piano_roll, "Piano Roll").clicked() {
                            self.layout.set_piano_roll_visible(piano_roll);
                        }
                        let mut plugin_rack = self.plugin_rack.visible;
                        if ui.checkbox(&mut plugin_rack, "Plugin Rack").clicked() {
                            self.plugin_rack.visible = plugin_rack;
                            self.layout.set_plugin_rack_visible(plugin_rack);
                        }
                    });
                });
            });
    }

    fn draw_audio_settings(&mut self, ui: &mut egui::Ui) {
        let realtime = self.engine_runner.realtime();
        self.audio_settings.update_active(realtime);
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
                let runtime_snapshot = self.engine_runner.runtime_options().clone();
                let realtime = self.engine_runner.realtime();
                self.audio_settings
                    .sync_with_runtime(&runtime_snapshot, realtime);
                if let Some(active) = realtime {
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

    fn draw_sequencer(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let pointer_released = ui.input(|i| i.pointer.button_released(PointerButton::Primary));
        let dragging_sample = matches!(self.drag_payload, Some(DragPayload::Sample(_)));
        self.section_label(ui, &self.icons.track, "Channel Rack");
        ui.add_space(10.0);

        if self.sequencer.instruments.is_empty() {
            Self::tinted_frame(&palette, ui, palette.panel_alt, |ui| {
                ui.label(
                    RichText::new("Add an instrument to begin sequencing.")
                        .color(palette.text_muted),
                );
                ui.label(
                    RichText::new("Each instrument can open the piano roll with a right click.")
                        .color(palette.text_muted)
                        .small(),
                );
            });
        } else {
            for index in 0..self.sequencer.instruments.len() {
                let fill = if self.focused_instrument == Some(index) {
                    palette.panel_alt.gamma_multiply(1.12)
                } else {
                    palette.panel_alt
                };
                Self::tinted_frame(&palette, ui, fill, |ui| {
                    self.draw_sequencer_instrument(ui, index);
                });
                ui.add_space(8.0);
            }
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.menu_button("Add Instrument", |ui| {
                for plugin in InstrumentPlugin::all() {
                    if ui
                        .button(plugin.display_name())
                        .on_hover_text(plugin.description())
                        .clicked()
                    {
                        self.add_sequencer_instrument(*plugin);
                        ui.close_menu();
                    }
                }
            });
            if ui.button("Refresh Clips").clicked() {
                self.refresh_sequencer_clips();
            }
        });

        let drop_rect = ui.min_rect();
        if dragging_sample {
            if let Some(pos) = pointer_pos {
                if drop_rect.contains(pos) {
                    ui.painter().rect_stroke(
                        drop_rect.expand(6.0),
                        12.0,
                        Stroke::new(2.0, palette.accent_alt.gamma_multiply(0.7)),
                    );
                }
            }
        }

        if pointer_released {
            if let Some(DragPayload::Sample(path)) = self.drag_payload.take() {
                if pointer_pos.map_or(false, |pos| drop_rect.contains(pos)) {
                    self.insert_sample_instrument(path);
                } else {
                    self.status_message = Some(
                        "Drag samples onto the channel rack to create sampler channels".into(),
                    );
                }
            }
        }
    }

    fn draw_sequencer_instrument(&mut self, ui: &mut egui::Ui, instrument_idx: usize) {
        if instrument_idx >= self.sequencer.instruments.len() {
            return;
        }

        let clip_reference = self.ensure_instrument_clip(instrument_idx);
        let palette = self.palette().clone();
        let instrument = self.sequencer.instruments[instrument_idx].clone();
        let accent = instrument.plugin.accent_color(&palette);

        let mut remove_requested = false;
        let header_response = ui
            .horizontal(|ui| {
                ui.label(
                    RichText::new(&instrument.name)
                        .color(palette.text_primary)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(instrument.plugin.display_name())
                        .color(palette.text_muted)
                        .small(),
                );
                ui.add_space(8.0);

                if self.tracks.is_empty() {
                    ui.label(
                        RichText::new("No mixer tracks available")
                            .color(palette.warning)
                            .small(),
                    );
                } else {
                    let mut target_track = instrument
                        .mixer_track
                        .min(self.tracks.len().saturating_sub(1));
                    let current_label = self
                        .tracks
                        .get(target_track)
                        .map(|track| track.name.clone())
                        .unwrap_or_else(|| "Unassigned".into());
                    egui::ComboBox::from_id_source(("sequencer_route", instrument.id))
                        .selected_text(current_label)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for (idx, track) in self.tracks.iter().enumerate() {
                                ui.selectable_value(
                                    &mut target_track,
                                    idx,
                                    format!("{:02} – {}", idx + 1, track.name),
                                );
                            }
                        });
                    if target_track != instrument.mixer_track {
                        self.assign_instrument_to_track(instrument_idx, target_track);
                    }
                }
            })
            .response;

        let header_response = header_response.on_hover_text(instrument.plugin.description());

        if header_response.clicked() {
            self.focused_instrument = Some(instrument_idx);
        }

        header_response.context_menu(|ui| {
            if let Some(reference) = clip_reference {
                if ui.button("Open Piano Roll").clicked() {
                    self.focus_clip(reference.track_index, reference.clip_index);
                    ui.close_menu();
                }
            }
            match instrument.plugin {
                InstrumentPlugin::WestCoastLead => {
                    if ui.button("Open Instrument Editor").clicked() {
                        self.westcoast_editor.open();
                        ui.close_menu();
                    }
                }
                InstrumentPlugin::Sub808 => {
                    if ui.button("Open Instrument Editor").clicked() {
                        self.sub808_editor.open();
                        ui.close_menu();
                    }
                }
                _ => {}
            }
            if let Some(selected_track) = self.selected_track {
                if selected_track < self.tracks.len() {
                    let label = format!("Route to {}", self.tracks[selected_track].name);
                    if ui.button(label).clicked() {
                        self.assign_instrument_to_track(instrument_idx, selected_track);
                        ui.close_menu();
                    }
                }
            }
            ui.separator();
            if ui.button("Remove Instrument").clicked() {
                remove_requested = true;
                ui.close_menu();
            }
        });

        if remove_requested {
            self.remove_sequencer_instrument(instrument_idx);
            return;
        }

        ui.add_space(6.0);

        if let Some(reference) = clip_reference {
            let track_label = self
                .tracks
                .get(reference.track_index)
                .map(|track| track.name.clone())
                .unwrap_or_else(|| "Mixer".into());
            ui.label(
                RichText::new(format!("Channel → {}", track_label))
                    .color(palette.text_muted)
                    .small(),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for step in 0..instrument.pattern.step_count {
                    if step > 0 && step % 4 == 0 {
                        ui.add_space(8.0);
                    }
                    let active = self.instrument_step_active(reference, &instrument.pattern, step);
                    let response =
                        Self::sequencer_step_widget(ui, active, accent, &palette, step % 4 == 0);
                    if response.clicked() {
                        self.toggle_instrument_step(instrument_idx, step, !active);
                        self.focused_instrument = Some(instrument_idx);
                    }
                    response.on_hover_text(format!("Step {}", step + 1));
                }
            });
        } else {
            ui.label(
                RichText::new("Clip unavailable for this instrument")
                    .color(palette.warning)
                    .small(),
            );
        }
    }

    fn sequencer_step_widget(
        ui: &mut egui::Ui,
        active: bool,
        accent: Color32,
        palette: &HarmoniqPalette,
        emphasise: bool,
    ) -> egui::Response {
        let base = if emphasise {
            palette.toolbar_highlight
        } else {
            palette.panel
        };
        let desired = egui::vec2(18.0, 32.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
        let mut fill = if active {
            accent
        } else {
            base.gamma_multiply(1.05)
        };
        if response.hovered() {
            fill = fill.gamma_multiply(1.08);
        }
        let stroke = if active {
            palette.accent_alt.gamma_multiply(1.1)
        } else {
            palette.toolbar_outline
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect.shrink(2.0), 4.0, fill);
        painter.rect_stroke(rect.shrink(2.0), 4.0, Stroke::new(1.0, stroke));
        response
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

                let pointer_hover = ui.input(|i| i.pointer.hover_pos());
                let pointer_pos = response.interact_pointer_pos().or(pointer_hover);
                let pointer_released =
                    ui.input(|i| i.pointer.button_released(PointerButton::Primary));
                let dragging_sample = matches!(self.drag_payload, Some(DragPayload::Sample(_)));
                let mut drop_target: Option<(usize, f32)> = None;
                let mut drop_indicator: Option<(f32, f32, f32)> = None;

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

                let playhead_x =
                    header_column_rect.right() + self.playback_position_beats * pixels_per_beat;
                if playhead_x >= timeline_rect.left() && playhead_x <= timeline_rect.right() {
                    painter.line_segment(
                        [
                            egui::pos2(playhead_x, ruler_rect.bottom()),
                            egui::pos2(playhead_x, timeline_rect.bottom()),
                        ],
                        Stroke::new(2.0, palette.accent_alt),
                    );
                }
                if self.loop_enabled {
                    let loop_end = self.bounce_length_beats.max(4.0);
                    let loop_start_x = header_column_rect.right();
                    let loop_end_x = header_column_rect.right() + loop_end * pixels_per_beat;
                    let loop_rect = egui::Rect::from_min_max(
                        egui::pos2(loop_start_x, ruler_rect.bottom() - 6.0),
                        egui::pos2(loop_end_x, ruler_rect.bottom()),
                    );
                    painter.rect_filled(loop_rect, 3.0, palette.accent_alt.gamma_multiply(0.2));
                }

                let clicked = response.clicked_by(PointerButton::Primary);
                let double_clicked = response.double_clicked_by(PointerButton::Primary);
                let right_clicked = response.clicked_by(PointerButton::Secondary);

                let mut cursor_y = timeline_rect.top() + row_gap;
                let selected_track = self.selected_track;
                let selected_clip = self.selected_clip;
                for (track_idx, track) in self.tracks.iter_mut().enumerate() {
                    let track_header_rect = egui::Rect::from_min_max(
                        egui::pos2(header_column_rect.left(), cursor_y),
                        egui::pos2(header_column_rect.right(), cursor_y + track_height),
                    );
                    let track_lane_rect = egui::Rect::from_min_max(
                        egui::pos2(timeline_rect.left(), cursor_y),
                        egui::pos2(timeline_rect.right(), cursor_y + track_height),
                    );
                    let is_selected = selected_track == Some(track_idx);

                    let pointer_over_lane = dragging_sample
                        && pointer_pos
                            .map(|pos| track_lane_rect.contains(pos))
                            .unwrap_or(false);
                    if pointer_over_lane {
                        if let Some(pos) = pointer_pos {
                            drop_target = Some((track_idx, pos.x));
                            drop_indicator =
                                Some((pos.x, track_lane_rect.top(), track_lane_rect.bottom()));
                        }
                    }

                    let header_fill = if pointer_over_lane {
                        palette.track_header_selected
                    } else if is_selected {
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
                    if pointer_over_lane {
                        painter.rect_stroke(
                            track_lane_rect,
                            0.0,
                            Stroke::new(1.5, palette.accent_alt.gamma_multiply(0.65)),
                        );
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
                        let clip_selected = selected_clip == Some((track_idx, clip_idx));
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
                        if clip.is_sample() {
                            painter.text(
                                egui::pos2(clip_rect.left() + 10.0, clip_rect.bottom() - 8.0),
                                Align2::LEFT_BOTTOM,
                                "Audio",
                                FontId::proportional(11.0),
                                palette.text_muted,
                            );
                        }
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

                if dragging_sample {
                    if let Some((x, top, bottom)) = drop_indicator {
                        let clamped_x = x.clamp(timeline_rect.left(), timeline_rect.right());
                        painter.line_segment(
                            [egui::pos2(clamped_x, top), egui::pos2(clamped_x, bottom)],
                            Stroke::new(2.0, palette.accent_soft.gamma_multiply(0.9)),
                        );
                    }
                }

                if pointer_released {
                    if let Some(DragPayload::Sample(path)) = self.drag_payload.take() {
                        if let Some((track_idx, drop_x)) = drop_target {
                            let beat = ((drop_x - timeline_rect.left()) / pixels_per_beat).max(0.0);
                            self.insert_sample_clip(track_idx, beat, path);
                        } else {
                            self.status_message = Some(
                                "Drag samples onto a playlist lane to create audio clips".into(),
                            );
                        }
                    }
                }
            });

        if let Some((track_idx, clip_idx)) = clip_to_select {
            self.focus_clip(track_idx, clip_idx);
        } else if let Some(track_idx) = track_to_select {
            if self.selected_track != Some(track_idx) {
                self.set_selected_clip(None);
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
            let piano_roll = self.piano_roll.clone();
            if let Some(track) = self.tracks.get_mut(track_idx) {
                let mut selected_note = self.piano_roll_selected_note;
                let mut drag_state = self.piano_roll_drag.take();
                let mut handled = false;
                if let Some(clip) = track.clips.get_mut(clip_idx) {
                    let desired_size = egui::vec2(ui.available_width(), ui.available_height());
                    let (response, painter) =
                        ui.allocate_painter(desired_size, egui::Sense::click_and_drag());

                    let rect = response.rect;
                    let key_height = piano_roll.key_height;
                    let pixels_per_beat = piano_roll.pixels_per_beat;
                    let num_keys = piano_roll.key_range_len();
                    let pointer_primary_pressed = ui.input(|i| i.pointer.primary_pressed());
                    let pointer_primary_down = ui.input(|i| i.pointer.primary_down());
                    let pointer_primary_released = ui.input(|i| i.pointer.primary_released());
                    let pointer_secondary_pressed = ui.input(|i| i.pointer.secondary_pressed());
                    let pointer_pos_hover = ui.input(|i| i.pointer.hover_pos());
                    let pointer_pos = response.interact_pointer_pos().or(pointer_pos_hover);

                    let mut keyboard_width = (rect.width() * 0.18).clamp(48.0, 96.0);
                    let gap = 6.0;
                    if rect.width() - keyboard_width - gap < 160.0 {
                        keyboard_width = (rect.width() - 160.0 - gap).clamp(32.0, 72.0);
                    }
                    if keyboard_width.is_nan() || keyboard_width <= 0.0 {
                        keyboard_width = 48.0;
                    }
                    let grid_left = (rect.left() + keyboard_width + gap).min(rect.right() - 40.0);
                    let keyboard_right = (grid_left - gap).max(rect.left());
                    let keyboard_rect = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(keyboard_right, rect.bottom()),
                    );
                    let grid_rect = if grid_left < rect.right() {
                        egui::Rect::from_min_max(egui::pos2(grid_left, rect.top()), rect.max)
                    } else {
                        rect
                    };
                    let separator_rect = egui::Rect::from_min_max(
                        egui::pos2(keyboard_right, rect.top()),
                        egui::pos2(grid_rect.left(), rect.bottom()),
                    );

                    let pointer_inside = pointer_pos.map_or(false, |pos| grid_rect.contains(pos));
                    let pointer_pitch =
                        pointer_pos.and_then(|pos| piano_roll.y_to_pitch(grid_rect, pos.y));

                    let clip_length = clip.length_beats.max(piano_roll.note_min_length());
                    let min_length = piano_roll.note_min_length();

                    painter.rect_filled(grid_rect, 6.0, palette.piano_background);
                    painter.rect_filled(keyboard_rect, 6.0, palette.panel_alt);
                    if separator_rect.width() > 0.0 {
                        painter.rect_filled(
                            separator_rect,
                            2.0,
                            palette.toolbar_outline.gamma_multiply(0.35),
                        );
                    }

                    for pitch in (*piano_roll.key_range.start()..=*piano_roll.key_range.end()).rev()
                    {
                        let bottom = piano_roll.pitch_to_y(grid_rect, pitch);
                        let top = bottom - key_height;
                        let lane_rect = egui::Rect::from_min_max(
                            egui::pos2(grid_rect.left(), top),
                            egui::pos2(grid_rect.right(), bottom),
                        );
                        let key_rect = egui::Rect::from_min_max(
                            egui::pos2(keyboard_rect.left(), top),
                            egui::pos2(keyboard_rect.right(), bottom),
                        );
                        let is_black = PianoRollState::is_black_key(pitch);
                        if is_black {
                            painter.rect_filled(
                                lane_rect,
                                0.0,
                                palette.piano_grid_minor.gamma_multiply(0.6),
                            );
                        } else if pitch % 12 == 0 {
                            painter.rect_filled(
                                lane_rect,
                                0.0,
                                palette.piano_background.gamma_multiply(1.15),
                            );
                        }
                        painter.rect_filled(
                            key_rect,
                            3.0,
                            if is_black {
                                palette.piano_black
                            } else {
                                palette.piano_white
                            },
                        );
                        painter.rect_stroke(
                            key_rect,
                            3.0,
                            Stroke::new(1.0, palette.timeline_border.gamma_multiply(0.5)),
                        );

                        if pitch % 12 == 0 {
                            let label = PianoRollState::note_label(pitch);
                            painter.text(
                                egui::pos2(key_rect.center().x, key_rect.center().y),
                                Align2::CENTER_CENTER,
                                label,
                                FontId::proportional(11.0),
                                if is_black {
                                    palette.piano_white
                                } else {
                                    palette.toolbar_outline
                                },
                            );
                        }
                    }

                    for i in 0..=num_keys {
                        let y = grid_rect.bottom() - key_height * i as f32;
                        painter.line_segment(
                            [
                                egui::pos2(grid_rect.left(), y),
                                egui::pos2(grid_rect.right(), y),
                            ],
                            egui::Stroke::new(1.0, palette.piano_grid_minor),
                        );
                    }

                    let total_beats = clip.length_beats.max(1.0).ceil() as usize;
                    for beat in 0..=total_beats * 4 {
                        let x = grid_rect.left() + beat as f32 * pixels_per_beat / 4.0;
                        let color = if beat % 4 == 0 {
                            palette.piano_grid_major
                        } else {
                            palette.piano_grid_minor
                        };
                        painter.line_segment(
                            [
                                egui::pos2(x, grid_rect.top()),
                                egui::pos2(x, grid_rect.bottom()),
                            ],
                            egui::Stroke::new(1.0, color),
                        );
                    }

                    for bar in 0..=total_beats {
                        let x = grid_rect.left() + bar as f32 * pixels_per_beat;
                        painter.text(
                            egui::pos2(x + pixels_per_beat * 0.5, grid_rect.top() + 4.0),
                            Align2::CENTER_TOP,
                            format!("{}", bar + 1),
                            FontId::proportional(11.0),
                            palette.text_muted,
                        );
                    }

                    if let Some(pitch_hover) = pointer_pitch {
                        if piano_roll.is_pitch_visible(pitch_hover) {
                            let bottom = piano_roll.pitch_to_y(grid_rect, pitch_hover);
                            let top = bottom - key_height;
                            let lane_rect = egui::Rect::from_min_max(
                                egui::pos2(grid_rect.left(), top),
                                egui::pos2(grid_rect.right(), bottom),
                            );
                            let key_rect = egui::Rect::from_min_max(
                                egui::pos2(keyboard_rect.left(), top),
                                egui::pos2(keyboard_rect.right(), bottom),
                            );
                            painter.rect_stroke(
                                lane_rect,
                                0.0,
                                Stroke::new(1.2, palette.accent.gamma_multiply(0.45)),
                            );
                            painter.rect_stroke(key_rect, 3.0, Stroke::new(1.6, palette.accent));
                        }
                    }

                    let mut note_rects: Vec<(usize, egui::Rect)> = Vec::new();
                    let mut hovered_note: Option<(usize, egui::Rect)> = None;
                    for (index, note) in clip.notes.iter().enumerate() {
                        if !piano_roll.is_pitch_visible(note.pitch) {
                            continue;
                        }
                        let x = grid_rect.left() + note.start_beats * pixels_per_beat;
                        let width = (note.length_beats * pixels_per_beat).max(10.0);
                        let y = piano_roll.pitch_to_y(grid_rect, note.pitch);
                        let note_rect = egui::Rect::from_min_size(
                            egui::pos2(x, y - key_height + 2.0),
                            egui::vec2(width, key_height - 4.0),
                        );
                        if pointer_pos
                            .map(|pos| note_rect.contains(pos))
                            .unwrap_or(false)
                        {
                            hovered_note = Some((index, note_rect));
                        }
                        let mut fill = clip.color.gamma_multiply(0.95);
                        if selected_note == Some(index) {
                            fill = fill.gamma_multiply(1.25);
                        } else if hovered_note
                            .map(|(hover_index, _)| hover_index == index)
                            .unwrap_or(false)
                        {
                            fill = fill.gamma_multiply(1.1);
                        }
                        painter.rect_filled(note_rect, 4.0, fill);
                        let border_color = if selected_note == Some(index) {
                            palette.clip_border_active
                        } else {
                            palette.timeline_border
                        };
                        painter.rect_stroke(note_rect, 4.0, egui::Stroke::new(1.0, border_color));
                        note_rects.push((index, note_rect));
                    }

                    if let Some((hover_index, hover_rect)) = hovered_note {
                        if pointer_inside {
                            let edge = 8.0;
                            if let Some(pos) = pointer_pos {
                                let near_left = (pos.x - hover_rect.left()).abs();
                                let near_right = (hover_rect.right() - pos.x).abs();
                                let cursor = if near_left <= edge || near_right <= edge {
                                    egui::CursorIcon::ResizeHorizontal
                                } else {
                                    egui::CursorIcon::Grab
                                };
                                ui.output_mut(|o| o.cursor_icon = cursor);
                            }
                        }
                        if selected_note.is_none() && pointer_primary_pressed {
                            selected_note = Some(hover_index);
                            ui.ctx().request_repaint();
                        }
                    }

                    if let Some(drag) = drag_state.as_mut() {
                        if let Some(note) = clip.notes.get_mut(drag.note_index) {
                            if let Some(pointer) = pointer_pos {
                                match drag.mode {
                                    PianoRollDragMode::Move => {
                                        let mut new_start = piano_roll
                                            .position_to_beat(grid_rect, pointer.x)
                                            - drag.drag_offset_beats;
                                        new_start = piano_roll.quantize_beat(new_start);
                                        let max_start = (clip_length - note.length_beats).max(0.0);
                                        note.start_beats = new_start.clamp(0.0, max_start);
                                        if let Some(pitch) =
                                            piano_roll.y_to_pitch(grid_rect, pointer.y)
                                        {
                                            note.pitch = pitch;
                                        }
                                    }
                                    PianoRollDragMode::ResizeStart => {
                                        let mut new_start = piano_roll.quantize_beat(
                                            piano_roll.position_to_beat(grid_rect, pointer.x),
                                        );
                                        let max_start = (drag.initial_end - min_length)
                                            .max(0.0)
                                            .min((clip_length - min_length).max(0.0));
                                        new_start = new_start.clamp(0.0, max_start);
                                        let mut new_length =
                                            (drag.initial_end - new_start).max(min_length);
                                        new_length = new_length
                                            .min((clip_length - new_start).max(min_length));
                                        note.start_beats = new_start;
                                        note.length_beats = new_length;
                                    }
                                    PianoRollDragMode::ResizeEnd | PianoRollDragMode::Create => {
                                        let mut new_end = piano_roll.quantize_beat(
                                            piano_roll.position_to_beat(grid_rect, pointer.x),
                                        );
                                        let min_end = note.start_beats + min_length;
                                        let max_end = clip_length.max(min_end);
                                        new_end = new_end.clamp(min_end, max_end);
                                        note.length_beats =
                                            (new_end - note.start_beats).max(min_length);
                                    }
                                }
                                ui.ctx().request_repaint();
                            }
                        } else {
                            drag_state = None;
                        }
                        if pointer_primary_released || !pointer_primary_down {
                            drag_state = None;
                        }
                    }

                    if drag_state.is_none() && pointer_secondary_pressed && pointer_inside {
                        if let Some((index, _)) = hovered_note {
                            clip.notes.remove(index);
                            if let Some(selected) = selected_note {
                                if selected == index {
                                    selected_note = None;
                                } else if selected > index {
                                    selected_note = Some(selected - 1);
                                }
                            }
                            drag_state = None;
                            ui.ctx().request_repaint();
                        }
                    }

                    if drag_state.is_none() && pointer_primary_pressed && pointer_inside {
                        if let Some((note_index, note_rect)) = hovered_note {
                            selected_note = Some(note_index);
                            ui.ctx().request_repaint();
                            if let Some(pointer) = pointer_pos {
                                let edge = 8.0;
                                let mode = if pointer.x <= note_rect.left() + edge {
                                    PianoRollDragMode::ResizeStart
                                } else if pointer.x >= note_rect.right() - edge {
                                    PianoRollDragMode::ResizeEnd
                                } else {
                                    PianoRollDragMode::Move
                                };
                                if let Some(note) = clip.notes.get(note_index) {
                                    let pointer_beat =
                                        piano_roll.position_to_beat(grid_rect, pointer.x);
                                    let drag_offset = pointer_beat - note.start_beats;
                                    drag_state = Some(PianoRollDragState {
                                        mode,
                                        note_index,
                                        drag_offset_beats: drag_offset,
                                        initial_start: note.start_beats,
                                        initial_length: note.length_beats,
                                        initial_end: note.start_beats + note.length_beats,
                                        initial_pitch: note.pitch,
                                    });
                                }
                            }
                        } else if let Some(pointer) = pointer_pos {
                            let mut beat = piano_roll
                                .quantize_beat(piano_roll.position_to_beat(grid_rect, pointer.x));
                            let max_start = (clip_length - min_length).max(0.0);
                            beat = beat.clamp(0.0, max_start);
                            let pitch = piano_roll
                                .y_to_pitch(grid_rect, pointer.y)
                                .unwrap_or(*piano_roll.key_range.end());
                            let length = min_length;
                            clip.notes.push(Note::new(beat, length, pitch));
                            let note_index = clip.notes.len() - 1;
                            selected_note = Some(note_index);
                            drag_state = Some(PianoRollDragState {
                                mode: PianoRollDragMode::Create,
                                note_index,
                                drag_offset_beats: 0.0,
                                initial_start: beat,
                                initial_length: length,
                                initial_end: beat + length,
                                initial_pitch: pitch,
                            });
                            ui.ctx().request_repaint();
                        }
                    }

                    if let Some(idx) = selected_note {
                        if idx >= clip.notes.len() {
                            selected_note = None;
                        }
                    }

                    handled = true;
                }
                self.piano_roll_selected_note = selected_note;
                self.piano_roll_drag = drag_state;
                if handled {
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

    fn volume_db_string(value: f32) -> String {
        if value <= 1e-5 {
            "-∞ dB".to_string()
        } else {
            let db = 20.0 * value.log10();
            format!("{:+.1} dB", db)
        }
    }

    fn draw_meter(
        ui: &mut egui::Ui,
        meter: &TrackMeter,
        palette: &HarmoniqPalette,
        desired_size: egui::Vec2,
    ) {
        let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, palette.meter_background);
        painter.rect_stroke(rect, 8.0, Stroke::new(1.0, palette.meter_border));

        let gutter = 4.0;
        let bar_width = (rect.width() - gutter * 3.0) / 2.0;
        let max_height = rect.height() - gutter * 2.0;
        let segments = [
            (0.0, 0.55, palette.meter_low),
            (0.55, 0.8, palette.meter_mid),
            (0.8, 0.95, palette.meter_high),
            (0.95, 1.0, palette.meter_peak),
        ];

        let draw_channel = |level: f32, x_start: f32, painter: &egui::Painter| {
            let level = level.clamp(0.0, 1.0);
            let x_end = x_start + bar_width;
            for &(start, end, color) in &segments {
                if level <= start {
                    continue;
                }
                let segment_end = level.min(end);
                if segment_end <= start {
                    continue;
                }
                let start_y = rect.bottom() - gutter - start * max_height;
                let end_y = rect.bottom() - gutter - segment_end * max_height;
                if end_y >= start_y {
                    continue;
                }
                let segment_rect = egui::Rect::from_min_max(
                    egui::pos2(x_start, end_y),
                    egui::pos2(x_end, start_y),
                );
                painter.rect_filled(segment_rect, 2.0, color);
            }
        };

        let left_start = rect.left() + gutter;
        let right_start = rect.left() + gutter * 2.0 + bar_width;
        draw_channel(meter.left_level(), left_start, &painter);
        draw_channel(meter.right_level(), right_start, &painter);

        let tick_color = palette.meter_border.gamma_multiply(0.6);
        for tick in [0.25_f32, 0.5, 0.75] {
            let y = rect.bottom() - gutter - tick * max_height;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + gutter * 0.6, y),
                    egui::pos2(rect.right() - gutter * 0.6, y),
                ],
                Stroke::new(0.5, tick_color),
            );
        }

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

    fn draw_strip_header(
        ui: &mut egui::Ui,
        label: &str,
        index: Option<usize>,
        accent: Color32,
        selected: bool,
        palette: &HarmoniqPalette,
    ) -> egui::Response {
        let width = ui.available_width().max(96.0);
        let header_height = 34.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, header_height), Sense::click());
        let painter = ui.painter_at(rect);
        let fill = if selected {
            palette.mixer_strip_header_selected
        } else {
            palette.mixer_strip_header
        };
        painter.rect_filled(rect, 8.0, fill);
        painter.rect_stroke(rect, 8.0, Stroke::new(1.0, palette.mixer_strip_border));

        let accent_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top()),
            egui::pos2(rect.left() + 3.0, rect.bottom()),
        );
        painter.rect_filled(
            accent_rect,
            Rounding {
                nw: 8.0,
                ne: 0.0,
                sw: 8.0,
                se: 0.0,
            },
            accent,
        );

        if let Some(index) = index {
            painter.text(
                egui::pos2(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                format!("{:02}", index + 1),
                FontId::proportional(11.0),
                palette.text_muted,
            );
        }

        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            palette.text_primary,
        );

        response
    }

    fn draw_strip_toggle(
        ui: &mut egui::Ui,
        value: &mut bool,
        label: &str,
        palette: &HarmoniqPalette,
    ) -> egui::Response {
        let size = egui::vec2(38.0, 20.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        let active = *value;
        let fill = if active {
            palette.mixer_toggle_active
        } else {
            palette.mixer_toggle_inactive
        };
        painter.rect_filled(rect, 6.0, fill);
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, palette.mixer_strip_border));
        let text_color = if active {
            palette.mixer_toggle_text
        } else {
            palette.text_muted
        };
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.0),
            text_color,
        );
        if response.clicked() {
            *value = !*value;
        }
        response
    }

    fn draw_led_toggle(
        ui: &mut egui::Ui,
        value: &mut bool,
        palette: &HarmoniqPalette,
    ) -> egui::Response {
        let size = egui::vec2(22.0, 22.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, palette.mixer_slot_bg);
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, palette.mixer_slot_border));
        let (led_color, glow) = if *value {
            (palette.meter_mid, palette.meter_low)
        } else {
            (
                palette.mixer_toggle_inactive.gamma_multiply(1.1),
                palette.mixer_slot_border,
            )
        };
        painter.circle_filled(rect.center(), 6.0, led_color);
        painter.circle_stroke(rect.center(), 6.0, Stroke::new(1.0, glow));
        if response.clicked() {
            *value = !*value;
        }
        response
    }

    fn draw_remove_button(ui: &mut egui::Ui, palette: &HarmoniqPalette) -> egui::Response {
        let size = egui::vec2(24.0, 22.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, palette.mixer_toggle_inactive);
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, palette.mixer_slot_border));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "✕",
            FontId::proportional(12.0),
            palette.warning,
        );
        response
    }

    fn draw_empty_effect_slot(ui: &mut egui::Ui, slot_index: usize, palette: &HarmoniqPalette) {
        let frame = egui::Frame::none()
            .fill(palette.mixer_slot_bg.gamma_multiply(0.85))
            .stroke(Stroke::new(1.0, palette.mixer_slot_border))
            .rounding(Rounding::same(10.0))
            .inner_margin(Margin::symmetric(10.0, 8.0));
        frame.show(ui, |ui| {
            ui.set_min_height(36.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:02}", slot_index + 1))
                        .small()
                        .color(palette.text_muted),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Empty Slot")
                        .italics()
                        .color(palette.text_muted),
                );
            });
        });
    }

    fn draw_effects_ui(
        track_index: Option<usize>,
        effects: &mut Vec<MixerEffect>,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
    ) -> bool {
        const MAX_INSERT_SLOTS: usize = 6;
        let mut removal: Option<usize> = None;
        let mut open_piano_roll = false;

        let total_effects = effects.len();
        let has_space_for_more = total_effects < MAX_INSERT_SLOTS;

        for (index, effect) in effects.iter_mut().enumerate() {
            let slot_fill = if effect.enabled {
                palette.mixer_slot_active
            } else {
                palette.mixer_slot_bg
            };
            let slot_frame = egui::Frame::none()
                .fill(slot_fill)
                .stroke(Stroke::new(1.0, palette.mixer_slot_border))
                .rounding(Rounding::same(10.0))
                .inner_margin(Margin::symmetric(12.0, 8.0));
            let response = slot_frame
                .show(ui, |ui| {
                    ui.set_min_height(40.0);
                    egui::Grid::new(format!("effect_slot_grid_{:?}_{index}", track_index))
                        .num_columns(2)
                        .spacing(egui::vec2(8.0, 4.0))
                        .min_col_width(0.0)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{:02} {}",
                                        index + 1,
                                        effect.effect_type.display_name()
                                    ))
                                    .color(if effect.enabled {
                                        palette.text_primary
                                    } else {
                                        palette.text_muted
                                    })
                                    .strong(),
                                );
                                ui.label(
                                    RichText::new(effect.effect_type.identifier())
                                        .small()
                                        .color(palette.text_muted),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if Self::draw_remove_button(ui, palette).clicked() {
                                        removal = Some(index);
                                    }
                                    ui.add_space(4.0);
                                    let _ = Self::draw_led_toggle(ui, &mut effect.enabled, palette);
                                },
                            );
                            ui.end_row();
                        });
                })
                .response;

            if track_index.is_some() {
                response.context_menu(|ui| {
                    if ui.button("Open Piano Roll").clicked() {
                        open_piano_roll = true;
                        ui.close_menu();
                    }
                });
            }

            if index + 1 < total_effects || has_space_for_more {
                ui.add_space(6.0);
            }
        }

        if let Some(index) = removal {
            effects.remove(index);
        }

        if effects.len() < MAX_INSERT_SLOTS {
            for slot in effects.len()..MAX_INSERT_SLOTS {
                Self::draw_empty_effect_slot(ui, slot, palette);
                if slot + 1 < MAX_INSERT_SLOTS {
                    ui.add_space(6.0);
                }
            }
        }

        ui.add_space(8.0);
        ui.menu_button(
            RichText::new("+ ADD INSERT")
                .strong()
                .extra_letter_spacing(3.0)
                .color(palette.accent),
            |ui| {
                for effect_type in EffectType::all() {
                    if ui.button(effect_type.display_name()).clicked() {
                        effects.push(MixerEffect::new(*effect_type));
                        ui.close_menu();
                    }
                }
            },
        );

        open_piano_roll && track_index.is_some()
    }

    fn draw_track_strip(
        ui: &mut egui::Ui,
        index: usize,
        track: &mut Track,
        is_selected: bool,
        palette: &HarmoniqPalette,
    ) -> (bool, bool) {
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
        frame.rounding = Rounding::same(10.0);
        let mut clicked = false;
        let mut request_piano_roll = false;
        frame.show(ui, |ui| {
            ui.set_min_width(104.0);
            ui.add_space(4.0);
            let accent = if track.solo {
                palette.success
            } else if track.muted {
                palette.warning
            } else if is_selected {
                palette.accent
            } else {
                palette.toolbar_outline
            };
            if Self::draw_strip_header(
                ui,
                track.name.as_str(),
                Some(index),
                accent,
                is_selected,
                palette,
            )
            .clicked()
            {
                clicked = true;
            }

            ui.add_space(6.0);
            let fader_height = 132.0;
            ui.horizontal(|ui| {
                ui.vertical_centered(|ui| {
                    Self::draw_meter(ui, &track.meter, palette, egui::vec2(18.0, fader_height));
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("{:.1} dB", track.meter.level_db()))
                            .small()
                            .color(palette.text_muted),
                    );
                });
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    ui.add(
                        Fader::new(&mut track.volume, 0.0, 1.5, 0.9, palette)
                            .with_height(fader_height),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(Self::volume_db_string(track.volume))
                            .small()
                            .color(palette.text_primary),
                    );
                });
            });

            ui.add_space(6.0);
            ui.centered_and_justified(|ui| {
                ui.add(
                    Knob::new(&mut track.pan, -1.0, 1.0, 0.0, "PAN", palette).with_diameter(40.0),
                );
            });

            ui.add_space(6.0);
            ui.centered_and_justified(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.horizontal(|ui| {
                    let _ = Self::draw_strip_toggle(ui, &mut track.muted, "M", palette);
                    let _ = Self::draw_strip_toggle(ui, &mut track.solo, "S", palette);
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            let header_text = RichText::new("INSERTS")
                .small()
                .extra_letter_spacing(2.0)
                .color(palette.text_muted);
            egui::CollapsingHeader::new(header_text)
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    if Self::draw_effects_ui(Some(index), &mut track.effects, ui, palette) {
                        request_piano_roll = true;
                    }
                });
        });
        ui.add_space(6.0);
        (clicked, request_piano_roll)
    }

    fn draw_master_strip(ui: &mut egui::Ui, master: &mut MasterChannel, palette: &HarmoniqPalette) {
        let mut frame = egui::Frame::group(ui.style());
        frame.fill = palette.mixer_strip_selected;
        frame.stroke = Stroke::new(1.0, palette.mixer_strip_border);
        frame.rounding = Rounding::same(10.0);
        frame.show(ui, |ui| {
            ui.set_min_width(120.0);
            ui.add_space(4.0);
            let _ = Self::draw_strip_header(
                ui,
                master.name.as_str(),
                None,
                palette.accent,
                true,
                palette,
            );

            ui.add_space(6.0);
            let fader_height = 144.0;
            ui.horizontal(|ui| {
                ui.vertical_centered(|ui| {
                    Self::draw_meter(ui, &master.meter, palette, egui::vec2(20.0, fader_height));
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("{:.1} dB", master.meter.level_db()))
                            .small()
                            .color(palette.text_muted),
                    );
                });
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.add(
                        Fader::new(&mut master.volume, 0.0, 1.5, 1.0, palette)
                            .with_height(fader_height),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(Self::volume_db_string(master.volume))
                            .small()
                            .color(palette.text_primary),
                    );
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            let header_text = RichText::new("MASTER INSERTS")
                .small()
                .extra_letter_spacing(2.0)
                .color(palette.text_muted);
            egui::CollapsingHeader::new(header_text)
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    let _ = Self::draw_effects_ui(None, &mut master.effects, ui, palette);
                });
        });
    }

    fn draw_mixer_console_contents(&mut self, ui: &mut egui::Ui) {
        self.update_mixer_visuals(ui.ctx());
        let palette = self.palette().clone();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Mixer Console")
                    .size(15.0)
                    .color(palette.text_primary)
                    .strong(),
            );
            if let Some(index) = self.selected_track {
                if let Some(track) = self.tracks.get(index) {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Selected: {}", track.name))
                            .small()
                            .color(palette.text_muted),
                    );
                }
            }
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(6.0);
        let mut new_selection = None;
        let mut piano_roll_request = None;
        egui::ScrollArea::both()
            .id_source("mixer_console_scroll")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for index in 0..self.tracks.len() {
                        let (clicked, request_piano_roll) = Self::draw_track_strip(
                            ui,
                            index,
                            &mut self.tracks[index],
                            self.selected_track == Some(index),
                            &palette,
                        );
                        if clicked {
                            new_selection = Some(index);
                        }
                        if request_piano_roll {
                            piano_roll_request = Some(index);
                        }
                    }
                    Self::draw_master_strip(ui, &mut self.master_track, &palette);
                });
            });
        if let Some(selection) = new_selection {
            self.selected_track = Some(selection);
        }
        if let Some(track_idx) = piano_roll_request {
            self.focus_piano_roll_on_track(track_idx);
        }
    }

    fn draw_mixer_window(&mut self, ctx: &egui::Context, palette: &HarmoniqPalette) {
        let mut open = self.mixer_console.show;
        let default_size = self.mixer_console.default_size;
        let min_size = self.mixer_console.min_size;

        if !open {
            self.mixer_console.show = false;
            return;
        }

        egui::Window::new("Mixer Console")
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_size(default_size)
            .min_size(min_size)
            .show(ctx, |ui| {
                ui.set_min_size(min_size);
                egui::Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(18.0))
                    .inner_margin(Margin::symmetric(18.0, 14.0))
                    .show(ui, |ui| {
                        self.draw_mixer_console_contents(ui);
                    });
            });

        self.mixer_console.show = open;
    }

    fn update_transport_clock(&mut self, ctx: &egui::Context) {
        let delta = ctx.input(|input| input.stable_dt).max(0.0);
        if matches!(
            self.transport_state,
            TransportState::Playing | TransportState::Recording
        ) {
            let beats_per_second = (self.tempo.max(1.0)) / 60.0;
            self.playback_position_beats += beats_per_second * delta;
            if self.loop_enabled && self.bounce_length_beats > 0.0 {
                let loop_length = self.bounce_length_beats.max(4.0);
                while self.playback_position_beats >= loop_length {
                    self.playback_position_beats -= loop_length;
                }
            }
        }
        self.transport_clock =
            TransportClock::from_beats(self.playback_position_beats, self.time_signature);
    }

    fn estimate_cpu_usage(&self) -> f32 {
        let track_load = self.tracks.len() as f32 * 0.005;
        let instrument_load = self.sequencer.instruments.len() as f32 * 0.01;
        let transport_load = if matches!(self.transport_state, TransportState::Playing) {
            0.08
        } else {
            0.0
        };
        (0.12 + track_load + instrument_load + transport_load).clamp(0.05, 0.95)
    }

    fn update_engine_context(&mut self) {
        if let Ok(mut ctx) = self.engine_context.lock() {
            ctx.tempo = self.tempo;
            ctx.time_signature = self.time_signature;
            ctx.transport = self.transport_state;
            ctx.pattern_mode = self.pattern_mode;
            ctx.cpu_usage = self.estimate_cpu_usage();
            ctx.clock = self.transport_clock;
            ctx.master_meter = (
                self.master_track.meter.left_level(),
                self.master_track.meter.right_level(),
            );
            ctx.ensure_demo_plugins();
            let external = self.external_plugins.summaries();
            ctx.plugins
                .retain(|plugin| !is_external_plugin_id(plugin.id));
            for summary in &external {
                ctx.plugins.push(PluginInstanceInfo::from_external(summary));
            }
            for (index, plugin) in ctx.plugins.iter_mut().enumerate() {
                let phase = self.playback_position_beats + index as f32 * 0.37;
                let dynamic = ((phase.sin() + 1.0) * 0.5 * 0.08).clamp(0.0, 0.25);
                plugin.cpu = (ctx.cpu_usage * 0.4 + dynamic).clamp(0.02, 0.95);
                plugin.latency_ms = 3.0 + index as f32 * 1.2;
            }
        }
    }

    fn browser_filter_matches(&self, path: &Path) -> bool {
        let filter = self.browser_panel.filter.trim();
        if filter.is_empty() {
            return true;
        }
        let needle = filter.to_ascii_lowercase();
        self.browser_filter_matches_recursive(path, &needle, 0)
    }

    fn browser_filter_matches_recursive(&self, path: &Path, needle: &str, depth: usize) -> bool {
        let name_matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase().contains(needle))
            .unwrap_or(false);
        if name_matches {
            return true;
        }
        if path.is_dir() && depth < 6 {
            if let Ok(read_dir) = fs::read_dir(path) {
                for entry in read_dir.flatten() {
                    if self.browser_filter_matches_recursive(&entry.path(), needle, depth + 1) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn draw_browser_directory(&mut self, ui: &mut egui::Ui, path: &Path, depth: usize) {
        if depth > 2 {
            return;
        }

        let Ok(read_dir) = fs::read_dir(path) else {
            if depth == 0 {
                ui.label(
                    RichText::new("Directory unavailable")
                        .color(self.palette().warning)
                        .italics(),
                );
            }
            return;
        };

        let mut entries: Vec<PathBuf> = read_dir
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in entries {
            if entry.is_dir() {
                if !self.browser_filter_matches(&entry) && !self.browser_panel.filter.is_empty() {
                    continue;
                }
                let label = entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Unnamed Folder")
                    .to_string();
                let header_id = egui::Id::new(("browser_dir", entry.clone()));
                egui::CollapsingHeader::new(label)
                    .id_source(header_id)
                    .default_open(depth == 0)
                    .show(ui, |ui| self.draw_browser_directory(ui, &entry, depth + 1));
            } else {
                self.draw_browser_file(ui, &entry);
            }
        }
    }

    fn draw_browser_file(&mut self, ui: &mut egui::Ui, path: &Path) {
        if !self.browser_filter_matches(path) {
            return;
        }
        let palette = self.palette().clone();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unnamed")
            .to_string();
        let is_selected = self
            .browser_panel
            .selected_file
            .as_ref()
            .map(|selected| selected == path)
            .unwrap_or(false);
        let is_favourite = self.browser_panel.favorites.contains(path);
        let label = if is_favourite {
            RichText::new(format!("★ {file_name}")).color(palette.accent)
        } else {
            RichText::new(file_name.clone()).color(palette.text_primary)
        };
        let response = ui.selectable_label(is_selected, label);
        if response.clicked() {
            self.browser_panel.selected_file = Some(path.to_path_buf());
        }
        if response.double_clicked() {
            self.status_message = Some(format!("Loaded {}", file_name));
        }
        if response.drag_started() {
            self.drag_payload = Some(DragPayload::Sample(path.to_path_buf()));
            ui.output_mut(|out| out.cursor_icon = CursorIcon::Grabbing);
        }
        response.context_menu(|ui| {
            let mut is_favourite = is_favourite;
            if ui.checkbox(&mut is_favourite, "Favourite").changed() {
                self.browser_panel.toggle_favourite(path);
            }
            if ui.button("Reveal").clicked() {
                self.status_message = Some(format!(
                    "Reveal not available in sandbox: {}",
                    path.display()
                ));
                ui.close_menu();
            }
        });
    }

    fn load_waveform_preview(&mut self, path: &Path) -> Option<WaveformPreview> {
        if let Some(preview) = self.browser_panel.waveform_cache.get(path) {
            return Some(preview.clone());
        }
        match generate_waveform_preview(path) {
            Ok(preview) => {
                self.browser_panel
                    .waveform_cache
                    .insert(path.to_path_buf(), preview);
                self.browser_panel.waveform_cache.get(path).cloned()
            }
            Err(err) => {
                let message = err.to_string();
                if !message.contains("unsupported audio format") {
                    self.last_error = Some(format!("Waveform preview error: {message}"));
                }
                None
            }
        }
    }

    fn draw_plugin_browser_tab(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette().clone();
        let filter = self.browser_panel.filter.trim().to_ascii_lowercase();
        for plugin in self.external_plugins.catalog() {
            if !filter.is_empty() && !plugin.name.to_ascii_lowercase().contains(&filter) {
                continue;
            }
            let label = format!("{} [{}]", plugin.name, plugin.display_format());
            let response = ui.selectable_label(false, label);
            if response.double_clicked() {
                self.load_external_plugin(plugin);
            }
            response.on_hover_text(plugin.path.display().to_string());
        }
        if self.external_plugins.catalog().is_empty() {
            ui.label(
                RichText::new("No plugins detected. Install VST3/LV2/CLAP components to begin.")
                    .color(palette.text_muted),
            );
        }
    }

    fn draw_waveform_preview(
        &mut self,
        ui: &mut egui::Ui,
        preview: &WaveformPreview,
        palette: &HarmoniqPalette,
    ) {
        let available = ui.available_width().max(160.0);
        let desired = egui::vec2(available, 120.0);
        let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, palette.panel_alt);
        painter.rect_stroke(rect, 8.0, Stroke::new(1.0, palette.toolbar_outline));

        if preview.samples.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No waveform",
                FontId::proportional(12.0),
                palette.text_muted,
            );
            return;
        }

        let sample_count = preview.samples.len();
        let step = (sample_count as f32 / rect.width().max(1.0)).max(1.0);
        let mut points = Vec::with_capacity(rect.width().ceil() as usize);
        let mid_y = rect.center().y;
        let amplitude = rect.height() * 0.45;
        let mut x = rect.left();
        let mut index: f32 = 0.0;
        while x <= rect.right() {
            let idx = index.floor() as usize;
            let sample = preview.samples.get(idx).copied().unwrap_or(0.0);
            let y = mid_y - sample * amplitude;
            points.push(egui::pos2(x, y));
            x += 1.0;
            index += step;
        }
        painter.add(egui::Shape::line(points, Stroke::new(1.6, palette.accent)));
        painter.text(
            egui::pos2(rect.left() + 8.0, rect.top() + 10.0),
            Align2::LEFT_TOP,
            format!("{:0.1} kHz", preview.sample_rate as f32 / 1000.0),
            FontId::proportional(11.0),
            palette.text_muted,
        );
    }

    fn draw_browser_panel(&mut self, ui: &mut egui::Ui) {
        self.browser_panel.refresh();
        let palette = self.palette().clone();
        ui.vertical(|ui| {
            ui.heading(RichText::new("Browser").color(palette.text_primary));
            ui.add_space(6.0);
            let filter_response = ui.add(
                egui::TextEdit::singleline(&mut self.browser_panel.filter)
                    .desired_width(f32::INFINITY)
                    .hint_text("Search samples, instruments, presets..."),
            );
            if filter_response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.command_palette.open();
            }
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_source("browser_scroll")
                .show(ui, |ui| {
                    for category in &mut self.browser_panel.categories {
                        let label = if category.path.exists() {
                            RichText::new(&category.name).color(palette.text_primary)
                        } else {
                            RichText::new(format!("{} (missing)", category.name))
                                .color(palette.warning)
                        };
                        let id = egui::Id::new(("browser_category", category.name.clone()));
                        let response = egui::CollapsingHeader::new(label)
                            .id_source(id)
                            .default_open(category.expanded);
                        if category.name == "Plugins" {
                            header.show(ui, |ui| self.draw_plugin_browser_tab(ui));
                        } else {
                            header
                                .show(ui, |ui| self.draw_browser_directory(ui, &category.path, 0));
                        }
                        category.expanded = !header.fully_closed();
                            .default_open(category.expanded)
                            .show(ui, |ui| self.draw_browser_directory(ui, &category.path, 0));
                        category.expanded = !response.fully_closed();
                        ui.add_space(4.0);
                    }
                });

            if let Some(selected) = self.browser_panel.selected_file.clone() {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Selection: {}",
                        selected
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Unknown")
                    ))
                    .color(palette.text_primary)
                    .strong(),
                );
                if let Some(preview) = self.load_waveform_preview(&selected) {
                    self.draw_waveform_preview(ui, &preview, &palette);
                } else {
                    ui.label(
                        RichText::new("Waveform preview unavailable for this file")
                            .color(palette.text_muted),
                    );
                }
            }
        });
    }

    fn draw_mixer_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(
            RichText::new("Mixer")
                .color(self.palette().text_primary)
                .size(18.0)
                .strong(),
        );
        ui.add_space(6.0);
        self.draw_mixer_console_contents(ui);
    }

    fn draw_plugin_rack(&mut self, ctx: &egui::Context) {
        if !self.plugin_rack.visible {
            return;
        }
        let mut open = self.plugin_rack.visible;
        egui::Window::new("Plugin Rack")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .default_pos(egui::pos2(1020.0, 120.0))
            .show(ctx, |ui| {
                let palette = self.palette().clone();
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Loaded Plugins").color(palette.text_primary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.plugin_rack.filter)
                                .hint_text("Filter")
                                .desired_width(160.0),
                        );
                    });
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_source("plugin_rack_scroll")
                    .show(ui, |ui| {
                        let mut engine_ctx = self.engine_context.lock();
                        for plugin in &mut engine_ctx.plugins {
                            if !self.plugin_rack.is_match(plugin) {
                                continue;
                            }
                            let id = plugin.id;
                            let cpu_text = format!("{:>4.1}%", plugin.cpu * 100.0);
                            let latency = format!("{:>4.1} ms", plugin.latency_ms);
                            let mut label = format!("{} • {}", plugin.name, plugin.plugin_type);
                            label.push_str(&format!(" | CPU {cpu_text} | Latency {latency}"));
                            let selected = self.plugin_rack.selected_plugin == Some(id);
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                self.plugin_rack.selected_plugin = Some(id);
                            }
                            if response.double_clicked() {
                                self.open_plugin_editor(plugin);
                            }
                            response.context_menu(|ui| {
                                if ui.checkbox(&mut plugin.bypassed, "Bypass").clicked() {
                                    self.plugin_rack.queue_bypass(id, plugin.bypassed);
                                }
                                if ui.button("Remove").clicked() {
                                    self.plugin_rack.queue_removal(id);
                                    ui.close_menu();
                                }
                            });
                        }
                    });
            });
        self.plugin_rack.visible = open;
        self.layout.set_plugin_rack_visible(open);
        self.process_plugin_actions();
    }

    fn draw_external_plugin_editors(&mut self, ctx: &egui::Context) {
        let ids = self.external_plugins.loaded_ids();
        for id in ids {
            let Some((name, format, is_open)) = self.external_plugins.editor_metadata(id) else {
                continue;
            };
            if !is_open {
                continue;
            }
            let mut open = is_open;
            let title = format!("{} ({})", name, format);
            egui::Window::new(title)
                .resizable(true)
                .default_width(360.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        if let Some(parameters) = self.external_plugins.parameters(id) {
                            for param in parameters.iter_mut() {
                                let mut value = param.value;
                                let slider = egui::Slider::new(&mut value, param.min..=param.max)
                                    .text(param.name.clone());
                                if ui.add(slider).changed() {
                                    self.external_plugins.set_parameter(id, param.index, value);
                                }
                            }
                        } else {
                            ui.label("No parameters available for this plugin");
                        }
                    });
                });
            if !open {
                self.external_plugins.set_editor_open(id, false);
            }
        }
    }

    fn open_plugin_editor(&mut self, plugin: &PluginInstanceInfo) {
        if let Some(external_id) = external_id_from_ui(plugin.id) {
            if let Some(_handle) = self.external_plugins.open_editor(external_id) {
                self.status_message = Some(format!("Opened {}", plugin.name));
            } else {
                self.status_message = Some(format!("{} does not expose an editor", plugin.name));
            }
            return;
        }
        match plugin.name.as_str() {
            "West Coast Lead" => self.westcoast_editor.open(),
            "Sub 808" => self.sub808_editor.open(),
            "Harmoniq Edison" => self.audio_editor.open(),
            _ => {
                self.status_message = Some(format!("Open editor for {}", plugin.name));
            }
        }
    }

    fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::TogglePianoRoll => {
                let visible = !self.layout.persistence().piano_roll_visible;
                self.layout.set_piano_roll_visible(visible);
            }
            CommandAction::ToggleBrowser => {
                let visible = !self.layout.persistence().browser_visible;
                self.layout.set_browser_visible(visible);
            }
            CommandAction::TogglePluginRack => {
                self.plugin_rack.visible = !self.plugin_rack.visible;
                self.layout
                    .set_plugin_rack_visible(self.plugin_rack.visible);
            }
            CommandAction::AddTrack => self.add_track(),
            CommandAction::AddInstrument => {
                self.add_sequencer_instrument(InstrumentPlugin::WestCoastLead)
            }
            CommandAction::SaveProject => self.save_project(),
            CommandAction::OpenProject => self.open_project(),
            CommandAction::BounceProject => self.bounce_project(),
            CommandAction::FocusMixer => self.mixer_console.open(),
            CommandAction::FocusPlaylist => self.focus_playlist_panel(),
            CommandAction::FocusChannelRack => self.focus_channel_rack_panel(),
            CommandAction::SetPatternMode => self.pattern_mode = true,
            CommandAction::SetSongMode => self.pattern_mode = false,
        }
    }

    fn focus_playlist_panel(&mut self) {
        if self.tracks.is_empty() {
            self.status_message = Some("No tracks available in the playlist".into());
            self.set_selected_clip(None);
            return;
        }
        let track_idx = self
            .selected_track
            .unwrap_or(0)
            .min(self.tracks.len().saturating_sub(1));
        self.selected_track = Some(track_idx);
        if self.tracks[track_idx].clips.is_empty() {
            self.set_selected_clip(None);
            self.status_message = Some(format!(
                "Playlist focused on {}",
                self.tracks[track_idx].name
            ));
        } else {
            self.focus_clip(track_idx, 0);
            self.status_message = Some(format!(
                "Playlist focused on {}",
                self.tracks[track_idx].name
            ));
        }
    }

    fn focus_channel_rack_panel(&mut self) {
        if self.sequencer.instruments.is_empty() {
            self.status_message = Some("Add an instrument to focus the channel rack".into());
            return;
        }
        let instrument_idx = self
            .focused_instrument
            .unwrap_or(0)
            .min(self.sequencer.instruments.len().saturating_sub(1));
        self.focused_instrument = Some(instrument_idx);
        let instrument = self.sequencer.instruments[instrument_idx].clone();
        if let Some(reference) = instrument.clip {
            if reference.track_index < self.tracks.len() {
                self.focus_clip(reference.track_index, reference.clip_index);
            }
        }
        self.status_message = Some(format!("Channel rack focused on {}", instrument.name));
    }

    fn load_external_plugin(&mut self, plugin: &DiscoveredPlugin) {
        match self.external_plugins.load(plugin) {
            Ok(id) => {
                self.status_message = Some(format!("Loaded {}", plugin.name));
                self.external_plugins.open_editor(id);
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.status_message = Some(format!("Failed to load {}", plugin.name));
            }
        }
    }

    fn process_plugin_actions(&mut self) {
        let (removals, bypasses) = self.plugin_rack.take_pending();
        if removals.is_empty() && bypasses.is_empty() {
            return;
        }
        if let Ok(mut engine_ctx) = self.engine_context.lock() {
            for (id, bypassed) in bypasses {
                if let Some(external_id) = external_id_from_ui(id) {
                    self.external_plugins.set_bypassed(external_id, bypassed);
                }
                if let Some(plugin) = engine_ctx.plugins.iter_mut().find(|p| p.id == id) {
                    plugin.bypassed = bypassed;
                    let state = if bypassed { "bypassed" } else { "active" };
                    self.status_message = Some(format!("{} set to {state}", plugin.name));
                }
            }
            for id in removals {
                if let Some(external_id) = external_id_from_ui(id) {
                    self.external_plugins.unload(external_id);
                }
                if let Some(index) = engine_ctx.plugins.iter().position(|p| p.id == id) {
                    let plugin = engine_ctx.plugins.remove(index);
                    self.status_message = Some(format!("Removed {}", plugin.name));
                }
        let mut engine_ctx = self.engine_context.lock();
        for (id, bypassed) in bypasses {
            if let Some(plugin) = engine_ctx.plugins.iter_mut().find(|p| p.id == id) {
                plugin.bypassed = bypassed;
                let state = if bypassed { "bypassed" } else { "active" };
                self.status_message = Some(format!("{} set to {state}", plugin.name));
            }
        }
        for id in removals {
            if let Some(index) = engine_ctx.plugins.iter().position(|p| p.id == id) {
                let plugin = engine_ctx.plugins.remove(index);
                self.status_message = Some(format!("Removed {}", plugin.name));
            }
        }
    }

    fn draw_command_palette(&mut self, ctx: &egui::Context) {
        if !self.command_palette.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Command Palette")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(420.0, 360.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let palette = self.palette().clone();
                ui.label(
                    RichText::new("Type a command")
                        .color(palette.text_muted)
                        .size(14.0),
                );
                let edit_id = ui.make_persistent_id("command_palette_input");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.command_palette.query)
                        .desired_width(f32::INFINITY)
                        .id(edit_id)
                        .hint_text("Search commands..."),
                );
                response.request_focus();
                if response.changed() {
                    self.command_palette.selected = 0;
                }

                ui.add_space(8.0);
                let filtered = self.command_palette.filtered_indices();
                let mut chosen: Option<CommandAction> = None;
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for (row, command_index) in filtered.iter().enumerate() {
                            let command = &self.command_palette.commands[*command_index];
                            let selected = self.command_palette.selected == row;
                            let label = if selected {
                                RichText::new(command.label).color(palette.accent)
                            } else {
                                RichText::new(command.label).color(palette.text_primary)
                            };
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                chosen = Some(command.action);
                            }
                            if response.hovered() {
                                self.command_palette.selected = row;
                            }
                            if response.double_clicked() {
                                chosen = Some(command.action);
                            }
                        }
                    });
                if filtered.is_empty() {
                    ui.label(RichText::new("No matches").color(palette.text_muted));
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Enter").color(palette.text_muted).monospace());
                    ui.label(RichText::new("to execute, Esc to cancel").color(palette.text_muted));
                });

                let mut select_index = self.command_palette.selected as isize;
                let total = filtered.len() as isize;
                if ctx.input(|i| i.key_pressed(Key::ArrowDown)) && total > 0 {
                    select_index = (select_index + 1).clamp(0, total - 1);
                    self.command_palette.selected = select_index as usize;
                }
                if ctx.input(|i| i.key_pressed(Key::ArrowUp)) && total > 0 {
                    select_index = (select_index - 1).clamp(0, total - 1);
                    self.command_palette.selected = select_index as usize;
                }
                if ctx.input(|i| i.key_pressed(Key::Enter)) && total > 0 {
                    let idx = filtered
                        .get(self.command_palette.selected)
                        .copied()
                        .and_then(|index| self.command_palette.commands.get(index))
                        .map(|command| command.action);
                    chosen = idx;
                }
                if ctx.input(|i| i.key_pressed(Key::Escape)) {
                    self.command_palette.close();
                }
                if let Some(action) = chosen {
                    self.execute_command(action);
                    self.command_palette.close();
                }
            });
        if !open {
            self.command_palette.close();
        }
    }
}

impl App for HarmoniqStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let palette = self.palette().clone();

        self.update_transport_clock(ctx);
        self.update_engine_context();

        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::P)))
            || ctx.input_mut(|i| {
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::P))
            })
        {
            self.command_palette.open();
        }

        let keyboard_events = self.typing_keyboard.collect_midi_events(ctx);
        if !keyboard_events.is_empty() {
            self.send_command(EngineCommand::SubmitMidi(keyboard_events));
        }

        egui::TopBottomPanel::top("main_menu")
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .outer_margin(Margin::symmetric(12.0, 4.0)),
            )
            .show(ctx, |ui| self.draw_main_menu(ui));

        egui::TopBottomPanel::top("transport")
            .frame(
                egui::Frame::none()
                    .fill(palette.background)
                    .outer_margin(Margin::symmetric(12.0, 10.0)),
            )
            .show(ctx, |ui| self.draw_transport_toolbar(ui));

        if self.layout.persistence().browser_visible {
            let panel = egui::SidePanel::left("browser_panel")
                .resizable(true)
                .default_width(self.layout.persistence().browser_width)
                .frame(
                    egui::Frame::none()
                        .fill(palette.panel)
                        .inner_margin(Margin::symmetric(16.0, 14.0))
                        .outer_margin(Margin::symmetric(12.0, 8.0))
                        .stroke(Stroke::new(1.0, palette.toolbar_outline))
                        .rounding(Rounding::same(16.0)),
                )
                .show(ctx, |ui| self.draw_browser_panel(ui));
            self.layout.set_browser_width(panel.response.rect.width());
        }
        let sequencer_panel = egui::SidePanel::left("channel_rack")
            .resizable(true)
            .default_width(self.layout.persistence().channel_rack_width)
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(18.0, 16.0))
                    .outer_margin(Margin::symmetric(12.0, 8.0))
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(18.0)),
            )
            .show(ctx, |ui| self.draw_sequencer(ui));
        self.layout
            .set_channel_rack_width(sequencer_panel.response.rect.width());

        let mixer_panel = egui::SidePanel::right("mixer_panel")
            .resizable(true)
            .default_width(self.layout.persistence().mixer_width)
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(18.0, 16.0))
                    .outer_margin(Margin::symmetric(12.0, 8.0))
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(18.0)),
            )
            .show(ctx, |ui| self.draw_mixer_panel(ui));
        self.layout
            .set_mixer_width(mixer_panel.response.rect.width());

        if self.layout.persistence().piano_roll_visible {
            let piano_panel = egui::TopBottomPanel::bottom("piano_roll")
                .resizable(true)
                .default_height(self.layout.persistence().piano_roll_height)
                .frame(
                    egui::Frame::none()
                        .fill(palette.panel)
                        .inner_margin(Margin::symmetric(18.0, 16.0))
                        .stroke(Stroke::new(1.0, palette.toolbar_outline))
                        .rounding(Rounding::same(18.0)),
                )
                .show(ctx, |ui| self.draw_piano_roll(ui));
            self.layout
                .set_piano_roll_height(piano_panel.response.rect.height());
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(18.0, 16.0))
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(Rounding::same(20.0)),
            )
            .show(ctx, |ui| self.draw_playlist(ui));

        self.draw_plugin_rack(ctx);
        self.draw_external_plugin_editors(ctx);
        self.westcoast_editor.draw(ctx, &palette);
        self.sub808_editor.draw(ctx, &palette);
        self.audio_editor.draw(ctx, &palette, &self.icons);
        if ctx.input(|i| i.pointer.button_released(PointerButton::Primary))
            && self.drag_payload.is_some()
        {
            self.drag_payload = None;
        }
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

        self.draw_command_palette(ctx);
        self.layout.maybe_save();
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

    fn insert_clip_sorted(&mut self, clip: Clip) -> usize {
        let key_name = clip.name.clone();
        let key_start = clip.start_beat;
        let key_source = clip.source_path.clone();
        self.clips.push(clip);
        self.clips.sort_by(|a, b| {
            a.start_beat
                .partial_cmp(&b.start_beat)
                .unwrap_or(Ordering::Equal)
        });
        self.clips
            .iter()
            .enumerate()
            .find(|(_, clip)| {
                clip.name == key_name
                    && (clip.start_beat - key_start).abs() <= f32::EPSILON * 4.0
                    && clip.source_path == key_source
            })
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| self.clips.len().saturating_sub(1))
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

const PROJECT_FILE_VERSION: u32 = 2;

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
    #[serde(default)]
    sequencer: SequencerState,
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
    #[serde(default)]
    source_path: Option<PathBuf>,
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
            source_path: clip.source_path.clone(),
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
            source_path: self.source_path,
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

struct Fader<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    default: f32,
    height: f32,
    palette: &'a HarmoniqPalette,
}

impl<'a> Fader<'a> {
    fn new(
        value: &'a mut f32,
        min: f32,
        max: f32,
        default: f32,
        palette: &'a HarmoniqPalette,
    ) -> Self {
        Self {
            value,
            min,
            max,
            default,
            height: 156.0,
            palette,
        }
    }

    fn with_height(mut self, height: f32) -> Self {
        self.height = height.max(80.0);
        self
    }
}

impl<'a> egui::Widget for Fader<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let width = 32.0;
        let (rect, mut response) =
            ui.allocate_exact_size(egui::vec2(width, self.height), Sense::click_and_drag());
        let mut value = (*self.value).clamp(self.min, self.max);

        if response.dragged() {
            let delta = ui.ctx().input(|i| i.pointer.delta().y);
            let sensitivity = (self.max - self.min).abs() / self.height.max(1.0);
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

        let track_rect = rect.shrink2(egui::vec2(width * 0.3, 10.0));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, self.palette.meter_background);
        painter.rect_stroke(rect, 8.0, Stroke::new(1.0, self.palette.meter_border));

        let normalized = (value - self.min) / (self.max - self.min).max(1e-6);
        let handle_y = track_rect.bottom() - normalized * track_rect.height();
        let handle_rect = egui::Rect::from_center_size(
            egui::pos2(track_rect.center().x, handle_y),
            egui::vec2(track_rect.width() + 6.0, 14.0),
        );

        painter.rect_filled(track_rect, 4.0, self.palette.toolbar_highlight);
        painter.rect_filled(handle_rect, 6.0, self.palette.accent);
        painter.rect_stroke(
            handle_rect,
            6.0,
            Stroke::new(1.0, self.palette.toolbar_outline),
        );

        response
    }
}

struct Knob<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    default: f32,
    label: &'a str,
    palette: &'a HarmoniqPalette,
    diameter: f32,
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
            diameter: 56.0,
        }
    }

    fn with_diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(28.0);
        self
    }
}

impl<'a> egui::Widget for Knob<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let knob_diameter = self.diameter;
        let label_height = 18.0;
        let desired_size = egui::vec2(knob_diameter + 16.0, knob_diameter + label_height + 12.0);
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

        let knob_radius = knob_diameter * 0.5;
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
    source_path: Option<PathBuf>,
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
            source_path: None,
        }
    }

    fn from_sample(
        name: impl Into<String>,
        start_beat: f32,
        length_beats: f32,
        color: Color32,
        source_path: PathBuf,
    ) -> Self {
        Self {
            name: name.into(),
            start_beat,
            length_beats,
            color,
            notes: Vec::new(),
            launch_state: ClipLaunchState::Stopped,
            source_path: Some(source_path),
        }
    }

    fn is_sample(&self) -> bool {
        self.source_path.is_some()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum InstrumentPlugin {
    SineSynth,
    WestCoastLead,
    Sub808,
    NoiseDrums,
    Sampler,
}

impl InstrumentPlugin {
    fn all() -> &'static [InstrumentPlugin] {
        &[
            InstrumentPlugin::SineSynth,
            InstrumentPlugin::WestCoastLead,
            InstrumentPlugin::Sub808,
            InstrumentPlugin::NoiseDrums,
            InstrumentPlugin::Sampler,
        ]
    }

    fn display_name(&self) -> &'static str {
        match self {
            InstrumentPlugin::SineSynth => "Sine Synth",
            InstrumentPlugin::WestCoastLead => "West Coast Lead",
            InstrumentPlugin::Sub808 => "Sub 808",
            InstrumentPlugin::NoiseDrums => "Noise Drums",
            InstrumentPlugin::Sampler => "Sampler",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            InstrumentPlugin::SineSynth => "Simple harmonic oscillator for lead or pad sketches",
            InstrumentPlugin::WestCoastLead => "Folded sine lead with LPG and modulation",
            InstrumentPlugin::Sub808 => "Punchy sub-bass generator with transient shaping",
            InstrumentPlugin::NoiseDrums => "Noise-based percussion generator for quick grooves",
            InstrumentPlugin::Sampler => "Trigger one-shot samples directly from the channel rack",
        }
    }

    fn default_pitch(&self) -> u8 {
        match self {
            InstrumentPlugin::SineSynth => 72,
            InstrumentPlugin::WestCoastLead => 76,
            InstrumentPlugin::Sub808 => 36,
            InstrumentPlugin::NoiseDrums => 38,
            InstrumentPlugin::Sampler => 60,
        }
    }

    fn default_pattern(&self) -> SequencerPattern {
        SequencerPattern::new(16, 0.25, self.default_pitch())
    }

    fn accent_color(&self, palette: &HarmoniqPalette) -> Color32 {
        match self {
            InstrumentPlugin::SineSynth => palette.accent_alt,
            InstrumentPlugin::WestCoastLead => palette.accent,
            InstrumentPlugin::Sub808 => palette.accent_soft,
            InstrumentPlugin::NoiseDrums => palette.accent.gamma_multiply(0.85),
            InstrumentPlugin::Sampler => palette.accent_alt.gamma_multiply(0.9),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ClipReference {
    track_index: usize,
    clip_index: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SequencerPattern {
    step_count: usize,
    step_length: f32,
    base_pitch: u8,
}

impl SequencerPattern {
    fn new(step_count: usize, step_length: f32, base_pitch: u8) -> Self {
        Self {
            step_count: step_count.max(1),
            step_length: step_length.max(0.03125),
            base_pitch,
        }
    }

    fn total_length(&self) -> f32 {
        self.step_length * self.step_count as f32
    }

    fn step_start(&self, index: usize) -> f32 {
        self.step_length * index as f32
    }

    fn tolerance(&self) -> f32 {
        (self.step_length * 0.25).max(0.002)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SequencerInstrument {
    id: usize,
    name: String,
    plugin: InstrumentPlugin,
    mixer_track: usize,
    pattern: SequencerPattern,
    clip: Option<ClipReference>,
    sample_path: Option<PathBuf>,
}

impl SequencerInstrument {
    fn new(
        id: usize,
        name: impl Into<String>,
        plugin: InstrumentPlugin,
        mixer_track: usize,
        pattern: SequencerPattern,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            plugin,
            mixer_track,
            pattern,
            clip: None,
            sample_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SequencerState {
    instruments: Vec<SequencerInstrument>,
    next_id: usize,
}

impl Default for SequencerState {
    fn default() -> Self {
        Self {
            instruments: Vec::new(),
            next_id: 1,
        }
    }
}

impl SequencerState {
    fn allocate_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn push_instrument(&mut self, instrument: SequencerInstrument) -> usize {
        self.instruments.push(instrument);
        self.instruments.len() - 1
    }

    fn next_name_for(&self, plugin: InstrumentPlugin) -> String {
        let count = self
            .instruments
            .iter()
            .filter(|instrument| instrument.plugin == plugin)
            .count()
            + 1;
        format!("{} {}", plugin.display_name(), count)
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

#[derive(Debug, Clone)]
struct PianoRollState {
    pixels_per_beat: f32,
    key_height: f32,
    key_range: std::ops::RangeInclusive<u8>,
    grid_division: f32,
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

    fn is_black_key(pitch: u8) -> bool {
        matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
    }

    fn note_label(pitch: u8) -> String {
        const NAMES: [&str; 12] = [
            "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
        ];
        let name = NAMES[(pitch % 12) as usize];
        let octave = (pitch / 12) as i32 - 1;
        format!("{}{}", name, octave)
    }

    fn position_to_beat(&self, rect: egui::Rect, x: f32) -> f32 {
        ((x - rect.left()) / self.pixels_per_beat).max(0.0)
    }

    fn quantize_beat(&self, beat: f32) -> f32 {
        if self.grid_division <= f32::EPSILON {
            beat
        } else {
            (beat / self.grid_division).round() * self.grid_division
        }
    }

    fn note_min_length(&self) -> f32 {
        self.grid_division.max(0.125)
    }
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            pixels_per_beat: 120.0,
            key_height: 18.0,
            key_range: 36..=84,
            grid_division: 0.25,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PianoRollDragMode {
    Move,
    ResizeStart,
    ResizeEnd,
    Create,
}

#[derive(Debug, Clone)]
struct PianoRollDragState {
    mode: PianoRollDragMode,
    note_index: usize,
    drag_offset_beats: f32,
    initial_start: f32,
    initial_length: f32,
    initial_end: f32,
    initial_pitch: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;
    use tempfile::tempdir;

    #[test]
    fn layout_persistence_roundtrip() {
        let dir = tempdir().expect("temp dir");
        let layout_path = dir.path().join("layout.json");
        std::env::set_var("HARMONIQ_UI_LAYOUT_PATH", layout_path.to_str().unwrap());

        let mut layout = LayoutState::load();
        layout.set_browser_width(360.0);
        layout.set_piano_roll_visible(false);
        layout.flush();

        let reloaded = LayoutState::load();
        std::env::remove_var("HARMONIQ_UI_LAYOUT_PATH");

        assert!((reloaded.persistence().browser_width - 360.0).abs() < 1e-3);
        assert!(!reloaded.persistence().piano_roll_visible);
    }

    #[test]
    fn waveform_preview_rejects_non_wav() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, b"not audio").expect("write");

        let result = generate_waveform_preview(&path);
        assert!(result.is_err());
    }

    #[test]
    fn insert_sample_clip_places_clip_on_track() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("clip.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).expect("writer");
        for _ in 0..48_000 {
            writer.write_sample(0i16).expect("sample");
        }
        writer.finalize().expect("finalize");

        let mut tracks = vec![Track::new("Track 01")];
        let color = Color32::from_rgb(128, 64, 192);
        let (index, warning) =
            insert_sample_clip_on_track(&mut tracks, 0, 3.2, path.clone(), 120.0, color)
                .expect("clip insertion");

        assert!(warning.is_none());
        let clip = &tracks[0].clips[index];
        assert!(clip.is_sample());
        assert!(clip
            .source_path
            .as_ref()
            .is_some_and(|stored| stored == &path));
        assert!((clip.start_beat - 3.25).abs() < 1e-3);
        assert!((clip.length_beats - 2.0).abs() < 0.01);
    }
}
