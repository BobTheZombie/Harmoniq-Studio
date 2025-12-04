use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use crossbeam::queue::ArrayQueue;
use parking_lot::{Mutex, RwLock};

use crate::audio_graph::{build_graph, GraphRunner};
use crate::mixer::api::{MixerUiApi, MixerUiState};
#[cfg(feature = "mixer_api")]
use crate::mixer::control::{
    ChannelId, EngineMixerHandle, MeterEvent, MixerBackend, SendId, MASTER_CHANNEL_ID,
};
use crate::mixer_rt::{AutoTx, Command, CommandTx, Mixer, MixerConfig, TrackId};
use crate::{
    automation::{
        AutomationCommand, AutomationEvent, AutomationLane, AutomationSender, CurveShape,
        ParameterSpec,
    },
    delay::DelayCompensator,
    graph::{GraphBuilder, GraphHandle},
    nodes::{GainNode as BuiltinGain, NodeNoise as BuiltinNoise, NodeOsc as BuiltinSine},
    plugin::{MidiEvent, PluginDescriptor, PluginId},
    rt::{AudioMetrics, AudioMetricsCollector},
    rt_bridge::RtBridge,
    scratch::RtAllocGuard,
    tone::ToneShaper,
    transport::Transport as TransportMetrics,
    AudioBuffer, AudioClip, AudioProcessor, BufferConfig,
};
use harmoniq_playlist::state::{AudioSourceId, ClipKind, PatternNote, Playlist};
use harmoniq_rt::RtEvent;
#[cfg(feature = "mixer_api")]
use log::debug;
use log::warn;

const COMMAND_QUEUE_CAPACITY: usize = 1024;
const METRICS_HISTORY_CAPACITY: usize = 512;

/// Transport state shared with UI and sequencing components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Playing,
    Recording,
}

impl Default for TransportState {
    fn default() -> Self {
        TransportState::Stopped
    }
}

/// Real-time safe command queue handle for communicating with the engine.
#[derive(Clone, Debug)]
pub struct EngineCommandQueue {
    queue: Arc<ArrayQueue<EngineCommand>>,
}

impl EngineCommandQueue {
    /// Attempts to push a command onto the queue without blocking.
    ///
    /// Returns the original command if the queue is full so callers can retry or
    /// degrade gracefully.
    pub fn try_send(&self, command: EngineCommand) -> Result<(), EngineCommand> {
        self.queue.push(command).map_err(|command| command)
    }

    /// Number of commands currently waiting to be processed.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Maximum capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Returns `true` when there are no pending commands.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Commands that can be sent to the engine from UI or automation.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    SetTempo(f32),
    SetTransport(TransportState),
    SetPatternMode(bool),
    SetPlaylist(Playlist),
    RegisterAudioSource(AudioSourceId, AudioClip),
    ReplaceGraph(GraphHandle),
    SubmitMidi(Vec<MidiEvent>),
    PlaySoundTest(AudioClip),
}

struct RtBlockSnapshot {
    graph: Option<GraphHandle>,
    plugin_ids: Vec<PluginId>,
    processors: Vec<Arc<Mutex<Box<dyn AudioProcessor>>>>,
    latencies: Vec<usize>,
    automation: Vec<Vec<AutomationEvent>>,
    pending_midi: Vec<MidiEvent>,
    max_latency: usize,
    graph_runner: Option<Mutex<GraphRunner>>,
}

impl Default for RtBlockSnapshot {
    fn default() -> Self {
        Self {
            graph: None,
            plugin_ids: Vec::new(),
            processors: Vec::new(),
            latencies: Vec::new(),
            automation: Vec::new(),
            pending_midi: Vec::new(),
            max_latency: 0,
            graph_runner: None,
        }
    }
}

/// Central Harmoniq engine responsible for orchestrating the processing graph.
pub struct HarmoniqEngine {
    config: BufferConfig,
    processors: RwLock<HashMap<PluginId, Arc<Mutex<Box<dyn AudioProcessor>>>>>,
    graph: RwLock<Option<GraphHandle>>,
    master_buffer: Mutex<AudioBuffer>,
    tone_shaper: ToneShaper,
    next_plugin_id: AtomicU64,
    transport: RwLock<TransportState>,
    pattern_mode: bool,
    tempo: f32,
    playlist: RwLock<Option<Playlist>>,
    playlist_last_tick: u64,
    playlist_audio: RwLock<HashMap<AudioSourceId, AudioClip>>,
    transport_metrics: Arc<TransportMetrics>,
    command_queue: Arc<ArrayQueue<EngineCommand>>,
    pending_midi: Mutex<Vec<MidiEvent>>,
    rt_snapshot: ArcSwap<RtBlockSnapshot>,
    automation_block: Vec<Vec<AutomationEvent>>,
    midi_block: Vec<MidiEvent>,
    automations: RwLock<HashMap<PluginId, AutomationLane>>,
    latencies: RwLock<HashMap<PluginId, usize>>,
    delay_lines: HashMap<PluginId, Box<DelayCompensator>>,
    sound_tests: Vec<ClipPlayback>,
    metrics: AudioMetricsCollector,
    block_period_ns: u64,
    automation_cursor: u64,
    mixer_cfg: MixerConfig,
    mixer: Mixer,
    mixer_command_tx: Mutex<CommandTx>,
    mixer_auto_tx: Mutex<AutoTx>,
    mixer_track_count: usize,
    mixer_warned_overflow: bool,
    #[cfg(feature = "mixer_api")]
    mixer_track_ids: Vec<ChannelId>,
    #[cfg(feature = "mixer_api")]
    mixer_channel_to_track: HashMap<ChannelId, TrackId>,
    mixer_ui: Arc<MixerUiState>,
    #[cfg(feature = "mixer_api")]
    mixer_handle: EngineMixerHandle,
    rt_bridge: Option<RtBridge>,
    last_reported_xruns: u64,
    last_reported_engine_load: u16,
    last_reported_max_block_us: u32,
}

impl HarmoniqEngine {
    pub fn new(config: BufferConfig) -> anyhow::Result<Self> {
        let command_queue = Arc::new(ArrayQueue::new(COMMAND_QUEUE_CAPACITY));
        let tone_shaper = ToneShaper::new(&config);
        let metrics = AudioMetricsCollector::new(METRICS_HISTORY_CAPACITY);
        let block_period_ns = Self::block_period_from_config(&config);
        let transport_metrics = Arc::new(TransportMetrics::default());
        transport_metrics
            .sr
            .store(config.sample_rate.round() as u32, Ordering::Relaxed);

        let mixer_cfg = MixerConfig {
            max_tracks: 64,
            max_block: config.block_size.max(1),
            sample_rate: config.sample_rate,
            smooth_alpha: 0.2,
            max_aux_busses: 4,
        };
        let (mixer, command_tx, auto_tx) = Mixer::new(mixer_cfg, 4096, 4096);
        let mixer_ui = MixerUiState::demo();
        #[cfg(feature = "mixer_api")]
        let mixer_handle = EngineMixerHandle::new(4096);
        let mut engine = Self {
            master_buffer: Mutex::new(AudioBuffer::from_config(&config)),
            processors: RwLock::new(HashMap::new()),
            graph: RwLock::new(None),
            next_plugin_id: AtomicU64::new(1),
            transport: RwLock::new(TransportState::Stopped),
            pattern_mode: true,
            tempo: 120.0,
            playlist: RwLock::new(None),
            playlist_last_tick: 0,
            playlist_audio: RwLock::new(HashMap::new()),
            transport_metrics: Arc::clone(&transport_metrics),
            command_queue,
            pending_midi: Mutex::new(Vec::new()),
            rt_snapshot: ArcSwap::from_pointee(RtBlockSnapshot::default()),
            automation_block: Vec::new(),
            midi_block: Vec::new(),
            config,
            tone_shaper,
            automations: RwLock::new(HashMap::new()),
            latencies: RwLock::new(HashMap::new()),
            delay_lines: HashMap::new(),
            sound_tests: Vec::new(),
            metrics,
            block_period_ns,
            automation_cursor: 0,
            mixer_cfg,
            mixer,
            mixer_command_tx: Mutex::new(command_tx),
            mixer_auto_tx: Mutex::new(auto_tx),
            mixer_track_count: 0,
            mixer_warned_overflow: false,
            #[cfg(feature = "mixer_api")]
            mixer_track_ids: Vec::new(),
            #[cfg(feature = "mixer_api")]
            mixer_channel_to_track: HashMap::new(),
            mixer_ui,
            #[cfg(feature = "mixer_api")]
            mixer_handle,
            rt_bridge: None,
            last_reported_xruns: 0,
            last_reported_engine_load: 0,
            last_reported_max_block_us: 0,
        };
        engine.install_default_graph()?;
        Ok(engine)
    }

    pub fn install_rt_bridge(&mut self, bridge: RtBridge) {
        self.rt_bridge = Some(bridge);
        self.last_reported_xruns = 0;
        self.last_reported_engine_load = 0;
        self.last_reported_max_block_us = 0;
    }

    pub fn config(&self) -> &BufferConfig {
        &self.config
    }

    pub fn graph(&self) -> Option<GraphHandle> {
        self.graph.read().clone()
    }

    pub fn plugin_descriptor(&self, id: PluginId) -> Option<PluginDescriptor> {
        let processors = self.processors.read();
        let handle = processors.get(&id)?.clone();
        drop(processors);
        let descriptor = handle.lock().descriptor();
        Some(descriptor)
    }

    pub fn reset_render_state(&mut self) -> anyhow::Result<()> {
        self.pending_midi.lock().clear();
        self.transport_metrics
            .sample_pos
            .store(0, Ordering::Relaxed);
        self.transport_metrics
            .playing
            .store(false, Ordering::Relaxed);
        self.automation_cursor = 0;
        self.metrics.reset();
        self.last_reported_xruns = 0;
        self.last_reported_engine_load = 0;
        self.last_reported_max_block_us = 0;
        self.sound_tests.clear();
        self.automation_block.clear();
        self.midi_block.clear();
        self.playlist_last_tick = 0;
        self.master_buffer.lock().clear();
        for delay in self.delay_lines.values_mut() {
            delay.reset();
        }

        {
            let processors = self.processors.read();
            for processor in processors.values() {
                processor.lock().prepare(&self.config)?;
            }
        }

        Ok(())
    }

    pub fn metrics(&self) -> AudioMetrics {
        self.metrics.snapshot()
    }

    pub fn metrics_collector(&self) -> AudioMetricsCollector {
        self.metrics.clone()
    }

    pub fn mixer_ui_api(&self) -> Arc<dyn MixerUiApi> {
        Arc::clone(&self.mixer_ui) as Arc<dyn MixerUiApi>
    }

    #[cfg(feature = "mixer_api")]
    pub fn mixer_handle(&self) -> &EngineMixerHandle {
        &self.mixer_handle
    }

    fn block_period_from_config(config: &BufferConfig) -> u64 {
        if config.block_size == 0 {
            return 0;
        }
        let sr = config.sample_rate.max(f32::EPSILON) as f64;
        let frames = config.block_size as f64;
        ((frames / sr) * 1_000_000_000.0).round() as u64
    }

    fn install_default_graph(&mut self) -> anyhow::Result<()> {
        if self.graph.read().is_some() {
            return Ok(());
        }

        let sine =
            self.register_processor(Box::new(BuiltinSine::new(220.0).with_amplitude(0.35)))?;
        let noise = self.register_processor(Box::new(BuiltinNoise::new(0.08)))?;
        let gain = self.register_processor(Box::new(BuiltinGain::new(0.6)))?;
        self.register_automation_parameter(gain, ParameterSpec::new(0, "Gain", 0.0, 2.0, 0.6))?;

        let mut builder = GraphBuilder::new();
        let sine_node = builder.add_node(sine);
        builder.connect_to_mixer(sine_node, 0.85)?;
        let noise_node = builder.add_node(noise);
        builder.connect_to_mixer(noise_node, 0.25)?;
        let gain_node = builder.add_node(gain);
        builder.connect_to_mixer(gain_node, 0.0)?;

        self.replace_graph(builder.build())
    }

    pub fn reconfigure(&mut self, config: BufferConfig) -> anyhow::Result<()> {
        let tone_enabled = self.tone_shaper.is_enabled();
        self.config = config.clone();
        self.master_buffer = Mutex::new(AudioBuffer::from_config(&config));
        self.tone_shaper = ToneShaper::new(&self.config);
        self.tone_shaper.set_enabled(tone_enabled);
        self.block_period_ns = Self::block_period_from_config(&self.config);
        self.metrics.reset();
        self.transport_metrics
            .sr
            .store(self.config.sample_rate.round() as u32, Ordering::Relaxed);

        self.mixer_cfg.max_block = self.config.block_size.max(1);
        self.mixer_cfg.sample_rate = self.config.sample_rate;
        let (mixer, command_tx, auto_tx) = Mixer::new(self.mixer_cfg, 4096, 4096);
        self.mixer = mixer;
        self.mixer_command_tx = Mutex::new(command_tx);
        self.mixer_auto_tx = Mutex::new(auto_tx);
        self.mixer_track_count = 0;
        self.mixer_warned_overflow = false;
        #[cfg(feature = "mixer_api")]
        {
            self.mixer_track_ids.clear();
            self.mixer_channel_to_track.clear();
        }

        {
            let processors = self.processors.read();
            let mut latencies = self.latencies.write();
            latencies.clear();
            for (id, processor) in processors.iter() {
                let mut processor = processor.lock();
                processor.prepare(&self.config)?;
                latencies.insert(*id, processor.latency_samples());
            }
        }

        self.delay_lines.clear();
        self.sound_tests.clear();
        self.automation_block.clear();
        self.midi_block.clear();
        let graph = { self.graph.read().clone() };
        if let Some(graph) = graph {
            self.configure_mixer_for_graph(&graph);
        }
        Ok(())
    }

    /// Enables or disables the built-in tone shaper. By default the engine
    /// keeps the shaper bypassed so that the master bus remains sonically
    /// neutral when no additional effects are loaded.
    pub fn set_tone_shaper_enabled(&mut self, enabled: bool) {
        self.tone_shaper.set_enabled(enabled);
    }

    /// Returns a lightweight handle that can be shared with UI threads for
    /// submitting commands.
    pub fn command_queue(&self) -> EngineCommandQueue {
        EngineCommandQueue {
            queue: Arc::clone(&self.command_queue),
        }
    }

    /// Attempts to enqueue a command directly on the engine's queue.
    pub fn try_enqueue_command(&self, command: EngineCommand) -> Result<(), EngineCommand> {
        self.command_queue.push(command).map_err(|command| command)
    }

    pub fn transport(&self) -> TransportState {
        *self.transport.read()
    }

    pub fn set_transport(&mut self, state: TransportState) {
        let mut current = self.transport.write();
        let was_playing = matches!(
            *current,
            TransportState::Playing | TransportState::Recording
        );
        let now_playing = matches!(state, TransportState::Playing | TransportState::Recording);
        *current = state;
        drop(current);

        self.transport_metrics
            .playing
            .store(now_playing, Ordering::Relaxed);
        if now_playing && !was_playing {
            self.transport_metrics
                .sample_pos
                .store(0, Ordering::Relaxed);
            self.automation_cursor = 0;
            self.playlist_last_tick = 0;
        }
        if !now_playing {
            self.transport_metrics
                .playing
                .store(false, Ordering::Relaxed);
            self.playlist_last_tick = 0;
        }
    }

    pub fn transport_metrics(&self) -> Arc<TransportMetrics> {
        Arc::clone(&self.transport_metrics)
    }

    pub fn register_processor(
        &mut self,
        mut processor: Box<dyn AudioProcessor>,
    ) -> anyhow::Result<PluginId> {
        processor.prepare(&self.config)?;
        let latency = processor.latency_samples();
        let id = PluginId(self.next_plugin_id.fetch_add(1, Ordering::SeqCst));
        let descriptor = processor.descriptor();
        tracing::info!("Registered processor: {}", descriptor);
        let shared = Arc::new(Mutex::new(processor));
        self.processors.write().insert(id, shared);
        self.latencies.write().insert(id, latency);
        self.delay_lines
            .insert(id, Box::new(DelayCompensator::new()));
        let mut lanes = self.automations.write();
        lanes.insert(id, AutomationLane::new(id, 1024));
        Ok(id)
    }

    pub fn replace_graph(&mut self, graph: GraphHandle) -> anyhow::Result<()> {
        if graph.is_empty() {
            anyhow::bail!("graph must contain at least one node");
        }
        self.configure_mixer_for_graph(&graph);
        *self.graph.write() = Some(graph);
        Ok(())
    }

    fn configure_mixer_for_graph(&mut self, graph: &GraphHandle) {
        let requested = graph.plugin_nodes().len();
        let max_tracks = self.mixer_cfg.max_tracks;
        let active = requested.min(max_tracks);
        if requested > max_tracks {
            if !self.mixer_warned_overflow {
                warn!(
                    "mixer only supports {} tracks; dropping excess (requested {})",
                    max_tracks, requested
                );
                self.mixer_warned_overflow = true;
            }
        } else {
            self.mixer_warned_overflow = false;
        }

        self.mixer_track_count = active;

        #[cfg(feature = "mixer_api")]
        {
            self.mixer_track_ids.clear();
            self.mixer_channel_to_track.clear();
            for idx in 0..active {
                let channel_id = (idx as ChannelId).saturating_add(1);
                self.mixer_track_ids.push(channel_id);
                self.mixer_channel_to_track
                    .insert(channel_id, idx as TrackId);
            }
        }

        let mut tx = self.mixer_command_tx.lock();
        let mut push = |cmd: Command| {
            if let Err(cmd) = tx.push(cmd) {
                warn!("dropping mixer command due to full queue: {:?}", cmd);
            }
        };

        for track in 0..max_tracks {
            if track > u16::MAX as usize {
                break;
            }
            let track_id = track as TrackId;
            let enable = track < active;
            push(Command::EnableTrack {
                track: track_id,
                enable,
            });
            if !enable {
                push(Command::SetMute {
                    track: track_id,
                    mute: false,
                });
                push(Command::SetSolo {
                    track: track_id,
                    solo: false,
                });
            }
        }

        for (idx, node) in graph.plugin_nodes().iter().take(active).enumerate() {
            if idx > u16::MAX as usize {
                break;
            }
            let track_id = idx as TrackId;
            let gain = graph.gain_for(*node);
            let gain_db = if gain <= 0.0 {
                -90.0
            } else {
                20.0 * gain.log10()
            };
            push(Command::SetGain {
                track: track_id,
                gain_db,
            });
            push(Command::SetPan {
                track: track_id,
                pan: 0.0,
            });
            push(Command::SetMute {
                track: track_id,
                mute: false,
            });
            push(Command::SetSolo {
                track: track_id,
                solo: false,
            });
        }

        push(Command::SetMasterGain { gain_db: 0.0 });
    }

    pub fn execute_command(&mut self, command: EngineCommand) -> anyhow::Result<()> {
        self.handle_command(command)
    }

    fn handle_command(&mut self, command: EngineCommand) -> anyhow::Result<()> {
        match command {
            EngineCommand::SetTempo(tempo) => {
                self.tempo = tempo.max(1.0);
            }
            EngineCommand::SetTransport(state) => self.set_transport(state),
            EngineCommand::SetPatternMode(enabled) => {
                self.pattern_mode = enabled;
                self.playlist_last_tick = 0;
            }
            EngineCommand::SetPlaylist(playlist) => {
                *self.playlist.write() = Some(playlist);
                self.playlist_last_tick = 0;
            }
            EngineCommand::RegisterAudioSource(id, clip) => {
                self.playlist_audio.write().insert(id, clip);
            }
            EngineCommand::ReplaceGraph(graph) => self.replace_graph(graph)?,
            EngineCommand::SubmitMidi(events) => {
                let mut pending = self.pending_midi.lock();
                pending.extend(events);
            }
            EngineCommand::PlaySoundTest(clip) => {
                self.sound_tests.push(ClipPlayback::new(clip));
            }
        }
        Ok(())
    }

    pub fn process_block(&mut self, output: &mut AudioBuffer) -> anyhow::Result<()> {
        if output.channel_count() != self.config.layout.channels() as usize
            || output.len() != self.config.block_size as usize
        {
            return Err(anyhow::anyhow!(
                "Output buffer does not match engine configuration"
            ));
        }

        self.render_block_with(|master, _| {
            for (target_channel, source_channel) in output.channels_mut().zip(master.channels()) {
                target_channel.copy_from_slice(source_channel);
            }
        })
    }

    pub(crate) fn render_block_with<R, F>(&mut self, mut visitor: F) -> anyhow::Result<R>
    where
        F: FnMut(&AudioBuffer, &[&AudioBuffer]) -> R,
    {
        let start = Instant::now();
        let period_ns = self.block_period_ns;
        let block_start = self.automation_cursor;
        let block_len = self.config.block_size as u32;
        let block_start_samples = self.transport_metrics.sample_pos.load(Ordering::Relaxed);
        let block_len_samples = self.config.block_size as u64;

        let result = (|| -> anyhow::Result<R> {
            self.prepare_block_snapshot(
                block_start,
                block_len,
                block_start_samples,
                block_len_samples,
            )?;

            let snapshot = self.rt_snapshot.load();
            if snapshot.graph.is_none() || snapshot.plugin_ids.is_empty() {
                let mut master = self.master_buffer.lock();
                master.clear();
                return Ok(visitor(&master, &[] as &[&AudioBuffer]));
            }

            let result = self.render_prepared_block(&snapshot, visitor)?;

            Ok(result)
        })();

        let elapsed = start.elapsed();
        self.metrics.record_block(elapsed, period_ns);
        self.emit_rt_metrics(elapsed, period_ns);
        if matches!(
            self.transport(),
            TransportState::Playing | TransportState::Recording
        ) {
            let rendered = self.config.block_size as u64;
            self.transport_metrics
                .sample_pos
                .fetch_add(rendered, Ordering::Relaxed);
            self.automation_cursor = self.automation_cursor.saturating_add(rendered);
        }
        result
    }

    fn emit_rt_metrics(&mut self, elapsed: Duration, period_ns: u64) {
        let Some(bridge) = self.rt_bridge.as_mut() else {
            return;
        };

        let snapshot = self.metrics.snapshot();

        if snapshot.xruns != self.last_reported_xruns {
            let count = snapshot.xruns.min(u64::from(u32::MAX)) as u32;
            bridge.push(RtEvent::Xrun { count });
            self.last_reported_xruns = snapshot.xruns;
        }

        if period_ns > 0 {
            let block_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
            let pct =
                ((u128::from(block_ns) * 1000) / u128::from(period_ns.max(1))).min(1000) as u16;
            if pct != self.last_reported_engine_load {
                bridge.push(RtEvent::EngineLoad { pct });
                self.last_reported_engine_load = pct;
            }
        }

        let max_block_us = (snapshot.max_block_ns / 1_000).min(u64::from(u32::MAX)) as u32;
        if max_block_us != 0 && max_block_us > self.last_reported_max_block_us {
            bridge.push(RtEvent::MaxBlockMicros { us: max_block_us });
            self.last_reported_max_block_us = max_block_us;
        }
    }

    fn process_playlist_block(
        &mut self,
        block_start_samples: u64,
        block_len_samples: u64,
        pending_midi: &mut Vec<MidiEvent>,
    ) {
        let Some(playlist) = self.playlist.read().clone() else {
            return;
        };

        let ppq = playlist.ppq().max(1);
        let samples_per_tick = self.samples_per_tick(ppq);
        if samples_per_tick <= 0.0 {
            return;
        }

        let block_start_tick = self.samples_to_ticks(block_start_samples, samples_per_tick);
        let block_end_tick = self.samples_to_ticks(
            block_start_samples.saturating_add(block_len_samples),
            samples_per_tick,
        );

        for (track_index, track) in playlist.tracks.iter().enumerate() {
            for lane in &track.lanes {
                for clip in &lane.clips {
                    if block_start_tick >= clip.end_ticks() || block_end_tick <= clip.start_ticks {
                        continue;
                    }

                    match clip.kind {
                        ClipKind::Pattern { pattern_id } => {
                            if let Some(pattern) = playlist.pattern(pattern_id) {
                                self.schedule_pattern_clip(
                                    clip.start_ticks,
                                    track_index,
                                    pattern,
                                    block_start_tick,
                                    block_end_tick,
                                    samples_per_tick,
                                    pending_midi,
                                    block_len_samples as u32,
                                );
                            }
                        }
                        ClipKind::Audio { source } => {
                            if block_start_tick <= clip.start_ticks
                                && clip.start_ticks < block_end_tick
                            {
                                self.trigger_audio_clip(source);
                            }
                        }
                        ClipKind::Automation { ref lane } => {
                            self.schedule_automation_clip(
                                track_index,
                                clip.start_ticks,
                                clip.duration_ticks,
                                lane,
                                block_start_tick,
                                block_end_tick,
                                block_start_samples,
                                samples_per_tick,
                            );
                        }
                    }
                }
            }
        }

        self.playlist_last_tick = block_end_tick;
    }

    fn schedule_automation_clip(
        &self,
        track_index: usize,
        clip_start_tick: u64,
        clip_duration_ticks: u64,
        lane: &harmoniq_playlist::state::AutomationLane,
        block_start_tick: u64,
        block_end_tick: u64,
        block_start_samples: u64,
        samples_per_tick: f64,
    ) {
        let graph = self.graph.read();
        let Some(graph_handle) = graph.as_ref() else {
            return;
        };
        let plugin_ids = graph_handle.plugin_ids();
        let Some(plugin_id) = plugin_ids.get(track_index).copied() else {
            return;
        };

        let Some(parameter) = self
            .automation_parameter_index(plugin_id, &lane.parameter)
            .or_else(|| {
                self.automation_parameter_spec(plugin_id, 0)
                    .map(|spec| spec.index)
            })
        else {
            return;
        };

        let Some(sender) = self.automation_sender(plugin_id) else {
            return;
        };

        for point in &lane.points {
            let absolute_tick = clip_start_tick.saturating_add(point.tick);
            if absolute_tick >= block_end_tick || absolute_tick < self.playlist_last_tick {
                continue;
            }
            if absolute_tick >= clip_start_tick.saturating_add(clip_duration_ticks) {
                continue;
            }

            if let Some(offset) = self.tick_to_offset(
                absolute_tick,
                block_start_tick,
                samples_per_tick,
                self.config.block_size as u32,
            ) {
                let sample = block_start_samples.saturating_add(offset as u64);
                let _ = sender.send(AutomationCommand::DrawCurve {
                    parameter,
                    sample,
                    value: point.value,
                    shape: CurveShape::Linear,
                });
            }
        }
    }

    fn schedule_pattern_clip(
        &self,
        clip_start_tick: u64,
        track_index: usize,
        pattern: &harmoniq_playlist::state::Pattern,
        block_start_tick: u64,
        block_end_tick: u64,
        samples_per_tick: f64,
        pending_midi: &mut Vec<MidiEvent>,
        block_len_samples: u32,
    ) {
        let track_channel = (track_index as u8) & 0x0F;

        for note in &pattern.notes {
            if note.duration_ticks <= 0 {
                continue;
            }

            let note_start_tick = clip_start_tick.saturating_add(note.start_ticks.max(0) as u64);
            let note_end_tick = note_start_tick.saturating_add(note.duration_ticks as u64);

            if note_start_tick >= block_start_tick && note_start_tick < block_end_tick {
                if let Some(offset) = self.tick_to_offset(
                    note_start_tick,
                    block_start_tick,
                    samples_per_tick,
                    block_len_samples,
                ) {
                    pending_midi.push(Self::note_event(note, offset, true, track_channel));
                }
            }

            if note_end_tick > block_start_tick && note_end_tick <= block_end_tick {
                if let Some(offset) = self.tick_to_offset(
                    note_end_tick,
                    block_start_tick,
                    samples_per_tick,
                    block_len_samples,
                ) {
                    pending_midi.push(Self::note_event(note, offset, false, track_channel));
                }
            }
        }
    }

    fn tick_to_offset(
        &self,
        tick: u64,
        block_start_tick: u64,
        samples_per_tick: f64,
        block_len_samples: u32,
    ) -> Option<u32> {
        let relative_ticks = tick.checked_sub(block_start_tick)? as f64;
        let sample_offset = (relative_ticks * samples_per_tick).round();
        if sample_offset < 0.0 {
            return None;
        }
        let offset = sample_offset as u64;
        if offset >= block_len_samples as u64 {
            None
        } else {
            Some(offset as u32)
        }
    }

    fn samples_per_tick(&self, ppq: u32) -> f64 {
        let tempo = self.tempo.max(f32::EPSILON) as f64;
        let sr = self.config.sample_rate.max(f32::EPSILON) as f64;
        (60.0 * sr) / (tempo * ppq as f64)
    }

    fn samples_to_ticks(&self, samples: u64, samples_per_tick: f64) -> u64 {
        (samples as f64 / samples_per_tick).floor() as u64
    }

    fn note_event(note: &PatternNote, sample_offset: u32, on: bool, channel: u8) -> MidiEvent {
        let status = (if on { 0x90 } else { 0x80 }) | (channel & 0x0F);
        let velocity = if on { note.velocity } else { 0 };
        MidiEvent::new(sample_offset, [status, note.pitch, velocity])
    }

    fn trigger_audio_clip(&mut self, source: AudioSourceId) {
        let Some(clip) = self.playlist_audio.read().get(&source).cloned() else {
            return;
        };
        self.sound_tests.push(ClipPlayback::new(clip));
    }

    fn process_mixer_block(
        mixer: &mut Mixer,
        mixer_cfg: MixerConfig,
        block_size: usize,
        track_buffers: &[AudioBuffer],
        master: &mut AudioBuffer,
    ) -> usize {
        let frames = track_buffers
            .first()
            .map(|buffer| buffer.len())
            .unwrap_or_else(|| {
                if master.len() != 0 {
                    master.len()
                } else {
                    block_size
                }
            });

        mixer.begin_block();

        if frames == 0 {
            master.clear();
            mixer.end_block();
            return 0;
        }

        if master.channel_count() != 2 || master.len() != frames {
            master.resize(2, frames);
        }

        let active = track_buffers.len().min(mixer_cfg.max_tracks);
        let mut inputs: Vec<Option<&[f32]>> = Vec::with_capacity(active);
        for buffer in track_buffers.iter().take(active) {
            if buffer.channel_count() == 0 || buffer.len() == 0 {
                inputs.push(None);
            } else {
                inputs.push(Some(buffer.channel(0)));
            }
        }

        let total_frames = frames.min(mixer_cfg.max_block);
        {
            let data = master.as_mut_slice();
            let (left, rest) = data.split_at_mut(total_frames);
            let (right, _) = rest.split_at_mut(total_frames.min(rest.len()));
            mixer.process(&inputs, left, right, total_frames);
        }

        mixer.end_block();
        active
    }

    #[cfg(feature = "mixer_api")]
    fn publish_mixer_track_meters(&mut self, track_count: usize) {
        let limit = track_count.min(self.mixer_track_ids.len());
        for idx in 0..limit {
            if idx > u16::MAX as usize {
                break;
            }
            let track_id = idx as TrackId;
            let Some(&channel_id) = self.mixer_track_ids.get(idx) else {
                continue;
            };
            let Some(peak) = self.mixer.track_peak(track_id) else {
                continue;
            };
            let rms = self.mixer.track_rms(track_id).unwrap_or(0.0);
            let event = MeterEvent {
                ch: channel_id,
                peak_l: peak,
                peak_r: peak,
                rms_l: rms,
                rms_r: rms,
                clip_l: peak >= 1.0,
                clip_r: peak >= 1.0,
            };
            self.mixer_handle.push_meter(event);
        }
    }

    #[cfg(feature = "mixer_api")]
    fn publish_master_meter(&mut self, buffer: &AudioBuffer) {
        let channels = buffer.channel_count();
        let frames = buffer.len();
        if channels == 0 || frames == 0 {
            return;
        }

        let left = buffer.channel(0);
        let right = if channels > 1 {
            buffer.channel(1)
        } else {
            left
        };

        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;
        let mut sum_sq_l = 0.0f32;
        let mut sum_sq_r = 0.0f32;

        for frame in 0..frames {
            let l = left[frame];
            let r = right[frame];
            peak_l = peak_l.max(l.abs());
            peak_r = peak_r.max(r.abs());
            sum_sq_l += l * l;
            sum_sq_r += r * r;
        }

        let inv_frames = 1.0 / frames as f32;
        let rms_l = (sum_sq_l * inv_frames).sqrt();
        let rms_r = (sum_sq_r * inv_frames).sqrt();

        let event = MeterEvent {
            ch: MASTER_CHANNEL_ID,
            peak_l,
            peak_r,
            rms_l,
            rms_r,
            clip_l: peak_l >= 1.0,
            clip_r: peak_r >= 1.0,
        };

        self.mixer_handle.push_meter(event);
    }

    fn drain_command_queue(&mut self) -> anyhow::Result<()> {
        while let Some(command) = self.command_queue.pop() {
            self.handle_command(command)?;
        }
        Ok(())
    }

    fn fill_automation_events_for_block(
        &mut self,
        plugin_ids: &[PluginId],
        block_start: u64,
        block_len: u32,
    ) {
        if block_len == 0 {
            self.automation_block.clear();
            return;
        }

        self.automation_block
            .resize_with(plugin_ids.len(), Vec::new);
        for bucket in &mut self.automation_block {
            bucket.clear();
        }

        let mut lanes = self.automations.write();
        for (index, plugin_id) in plugin_ids.iter().enumerate() {
            if let Some(lane) = lanes.get_mut(plugin_id) {
                lane.render(block_start, block_len, &mut self.automation_block[index]);
            }
        }
    }

    fn prepare_block_snapshot(
        &mut self,
        block_start: u64,
        block_len: u32,
        block_start_samples: u64,
        block_len_samples: u64,
    ) -> anyhow::Result<()> {
        self.drain_command_queue()?;

        self.midi_block.clear();
        {
            let mut queue = self.pending_midi.lock();
            self.midi_block.extend(queue.drain(..));
        }

        let mut midi_block = core::mem::take(&mut self.midi_block);

        if !self.pattern_mode
            && matches!(
                self.transport(),
                TransportState::Playing | TransportState::Recording
            )
        {
            self.process_playlist_block(block_start_samples, block_len_samples, &mut midi_block);
        }

        #[cfg(feature = "mixer_api")]
        {
            let mut adapter = MixerUiBridge::new(
                Arc::clone(&self.mixer_ui),
                &self.mixer_command_tx,
                &self.mixer_channel_to_track,
            );
            self.mixer_handle.drain_commands_and_apply(&mut adapter);
        }

        let graph = self.graph.read().clone();
        let Some(graph) = graph else {
            self.midi_block = midi_block;
            self.rt_snapshot.store(Arc::new(RtBlockSnapshot::default()));
            return Ok(());
        };

        let plugin_ids = graph.plugin_ids();
        if plugin_ids.is_empty() {
            self.midi_block = midi_block;
            self.rt_snapshot.store(Arc::new(RtBlockSnapshot::default()));
            return Ok(());
        }

        let processors_guard = self.processors.read();
        let processor_handles: Vec<_> = plugin_ids
            .iter()
            .map(|plugin_id| {
                processors_guard.get(plugin_id).cloned().ok_or_else(|| {
                    anyhow::anyhow!("Missing processor for plugin ID: {:?}", plugin_id)
                })
            })
            .collect::<anyhow::Result<_>>()?;
        drop(processors_guard);

        let latencies_guard = self.latencies.read();
        let latencies: Vec<usize> = plugin_ids
            .iter()
            .map(|plugin_id| *latencies_guard.get(plugin_id).unwrap_or(&0))
            .collect();
        drop(latencies_guard);

        self.fill_automation_events_for_block(&plugin_ids, block_start, block_len);
        let max_latency = latencies.iter().copied().max().unwrap_or(0);

        let mixer_ptr = NonNull::from(&mut self.mixer);
        let runner = build_graph(
            &plugin_ids,
            &processor_handles,
            &latencies,
            &self.automation_block,
            &midi_block,
            mixer_ptr,
            self.mixer_cfg,
            &mut self.delay_lines,
            self.config.layout.channels() as usize,
            self.config.block_size,
        );

        let snapshot = RtBlockSnapshot {
            graph: Some(graph),
            plugin_ids,
            processors: processor_handles,
            latencies,
            automation: self.automation_block.clone(),
            pending_midi: midi_block.clone(),
            max_latency,
            graph_runner: Some(Mutex::new(runner)),
        };

        self.midi_block = midi_block;
        self.rt_snapshot.store(Arc::new(snapshot));
        Ok(())
    }

    fn render_prepared_block<R, F>(
        &mut self,
        snapshot: &RtBlockSnapshot,
        mut visitor: F,
    ) -> anyhow::Result<R>
    where
        F: FnMut(&AudioBuffer, &[&AudioBuffer]) -> R,
    {
        let Some(runner_mutex) = snapshot.graph_runner.as_ref() else {
            let mut master = self.master_buffer.lock();
            master.clear();
            return Ok(visitor(&master, &[] as &[&AudioBuffer]));
        };

        let frames = self.config.block_size.max(1);

        {
            let mut runner = runner_mutex.lock();
            runner.process(frames)?;

            let master_src = runner.master();
            let mut master = self.master_buffer.lock();
            if master.channel_count() != master_src.channel_count()
                || master.len() != master_src.len()
            {
                master.resize(master_src.channel_count(), master_src.len());
            }
            let dst = master.as_mut_slice();
            let src = master_src.as_slice();
            let len = dst.len().min(src.len());
            dst[..len].copy_from_slice(&src[..len]);

            let _guard = RtAllocGuard::enter();
            self.tone_shaper.process(&mut master);

            let mut index = 0;
            while index < self.sound_tests.len() {
                if self.sound_tests[index].mix_into(&mut master) {
                    self.sound_tests.remove(index);
                } else {
                    index += 1;
                }
            }

            #[cfg(feature = "mixer_api")]
            {
                let active_tracks = snapshot.plugin_ids.len().min(self.mixer_cfg.max_tracks);
                self.publish_mixer_track_meters(active_tracks);
                self.publish_master_meter(&master);
            }

            let outputs: Vec<&AudioBuffer> = runner.node_outputs();
            return Ok(visitor(&master, &outputs));
        }
    }
}

#[cfg(feature = "mixer_api")]
struct MixerUiBridge<'a> {
    state: Arc<MixerUiState>,
    command_tx: &'a Mutex<CommandTx>,
    channel_to_track: &'a HashMap<ChannelId, TrackId>,
}

#[cfg(feature = "mixer_api")]
impl<'a> MixerUiBridge<'a> {
    fn new(
        state: Arc<MixerUiState>,
        command_tx: &'a Mutex<CommandTx>,
        channel_to_track: &'a HashMap<ChannelId, TrackId>,
    ) -> Self {
        Self {
            state,
            command_tx,
            channel_to_track,
        }
    }

    fn strip_index(&self, channel: ChannelId) -> Option<usize> {
        let len = self.state.strips_len();
        (0..len).find(|&idx| self.state.strip_info(idx).id == channel)
    }

    fn track_id(&self, channel: ChannelId) -> Option<TrackId> {
        self.channel_to_track.get(&channel).copied()
    }

    fn push_command(&self, command: Command) {
        let mut tx = self.command_tx.lock();
        if let Err(cmd) = tx.push(command) {
            warn!("dropping mixer command due to full queue: {:?}", cmd);
        }
    }

    fn set_fader_and_pan(&self, idx: usize, gain_db: f32, pan: f32) {
        self.state.set_fader_db(idx, gain_db);
        self.state.set_pan(idx, pan);
    }

    fn set_mute_state(&self, idx: usize, mute: bool) {
        let current = self.state.strip_info(idx).muted;
        if current != mute {
            self.state.toggle_mute(idx);
        }
    }

    fn set_solo_state(&self, idx: usize, solo: bool) {
        let current = self.state.strip_info(idx).soloed;
        if current != solo {
            self.state.toggle_solo(idx);
        }
    }

    fn set_insert_bypass(&self, idx: usize, slot: usize, bypass: bool) {
        if self.state.insert_is_bypassed(idx, slot) != bypass {
            self.state.insert_toggle_bypass(idx, slot);
        }
    }

    fn set_send_level(&self, idx: usize, slot: usize, level: f32) {
        let level = level.max(1e-6);
        let db = 20.0 * level.log10();
        self.state.send_set_level(idx, slot, db);
    }
}

#[cfg(feature = "mixer_api")]
impl<'a> MixerBackend for MixerUiBridge<'a> {
    fn set_gain_pan(&mut self, ch: ChannelId, gain_db: f32, pan: f32) {
        if let Some(idx) = self.strip_index(ch) {
            let (gain_l, gain_r) = crate::panlaw::constant_power_pan_gains(pan);
            debug!(channel = ch, gain_l, gain_r, "constant-power pan gains");
            self.set_fader_and_pan(idx, gain_db, pan);
        }
        if let Some(track) = self.track_id(ch) {
            self.push_command(Command::SetGain { track, gain_db });
            self.push_command(Command::SetPan { track, pan });
        }
    }

    fn set_mute(&mut self, ch: ChannelId, mute: bool) {
        if let Some(idx) = self.strip_index(ch) {
            self.set_mute_state(idx, mute);
        }
        if let Some(track) = self.track_id(ch) {
            self.push_command(Command::SetMute { track, mute });
        }
    }

    fn set_solo(&mut self, ch: ChannelId, solo: bool) {
        if let Some(idx) = self.strip_index(ch) {
            self.set_solo_state(idx, solo);
        }
        if let Some(track) = self.track_id(ch) {
            self.push_command(Command::SetSolo { track, solo });
        }
    }

    fn open_insert_browser(&mut self, ch: ChannelId, slot: Option<usize>) {
        debug!(channel = ch, ?slot, "open_insert_browser request");
    }

    fn open_insert_ui(&mut self, ch: ChannelId, slot: usize) {
        debug!(channel = ch, slot, "open_insert_ui request");
    }

    fn set_insert_bypass(&mut self, ch: ChannelId, slot: usize, bypass: bool) {
        if let Some(idx) = self.strip_index(ch) {
            self.set_insert_bypass(idx, slot, bypass);
        }
    }

    fn remove_insert(&mut self, ch: ChannelId, slot: usize) {
        warn!(channel = ch, slot, "remove_insert not implemented");
    }

    fn configure_send(&mut self, ch: ChannelId, id: SendId, level: f32, pre_fader: bool) {
        if let Some(idx) = self.strip_index(ch) {
            self.set_send_level(idx, id as usize, level);
            if pre_fader {
                trace!(
                    channel = ch,
                    id,
                    "pre-fader send requested (UI bridge placeholder)"
                );
            }
        }
    }

    fn set_stereo_separation(&mut self, ch: ChannelId, amount: f32) {
        if let Some(idx) = self.strip_index(ch) {
            self.state.set_width(idx, amount);
        }
    }

    fn reorder_insert(&mut self, ch: ChannelId, from: usize, to: usize) {
        if let Some(idx) = self.strip_index(ch) {
            self.state.insert_move(idx, from, to);
        }
    }

    fn apply_routing(&mut self, set: &[(ChannelId, String, f32)], remove: &[(ChannelId, String)]) {
        if !set.is_empty() {
            debug!(?set, "apply_routing set requests (UI bridge placeholder)");
        }
        if !remove.is_empty() {
            debug!(
                ?remove,
                "apply_routing remove requests (UI bridge placeholder)"
            );
        }
    }
}

struct ClipPlayback {
    clip: AudioClip,
    position: usize,
}

impl ClipPlayback {
    fn new(clip: AudioClip) -> Self {
        Self { clip, position: 0 }
    }

    fn mix_into(&mut self, buffer: &mut AudioBuffer) -> bool {
        let total_frames = self.clip.frames();
        if total_frames == 0 {
            return true;
        }

        let clip_channels = self.clip.channels();
        if clip_channels == 0 {
            return true;
        }

        let clip_samples = self.clip.samples();
        let mut position = self.position;
        let channel_count = buffer.channel_count();
        if channel_count == 0 {
            return true;
        }

        let available_frames = buffer.len();
        if available_frames == 0 {
            return true;
        }

        let data = buffer.as_mut_slice();

        for frame_index in 0..available_frames {
            if position >= total_frames {
                break;
            }

            for channel_index in 0..channel_count {
                let source_channel = channel_index.min(clip_channels - 1);
                if let Some(value) = clip_samples
                    .get(source_channel)
                    .and_then(|channel| channel.get(position))
                {
                    let dest_index = channel_index * available_frames + frame_index;
                    if dest_index < data.len() {
                        data[dest_index] += *value;
                    }
                }
            }

            position += 1;
        }

        self.position = position;
        self.position >= total_frames
    }
}

impl HarmoniqEngine {
    pub fn automation_sender(&self, plugin_id: PluginId) -> Option<AutomationSender> {
        self.automations
            .read()
            .get(&plugin_id)
            .map(|lane| lane.sender())
    }

    pub fn register_automation_parameter(
        &self,
        plugin_id: PluginId,
        spec: ParameterSpec,
    ) -> anyhow::Result<()> {
        let mut lanes = self.automations.write();
        let lane = lanes
            .get_mut(&plugin_id)
            .ok_or_else(|| anyhow::anyhow!("missing automation lane for plugin"))?;
        lane.register_parameter(spec);
        Ok(())
    }

    pub fn automation_parameter_index(&self, plugin_id: PluginId, name: &str) -> Option<usize> {
        self.automations
            .read()
            .get(&plugin_id)
            .and_then(|lane| lane.parameter_index_by_name(name))
    }

    pub fn automation_parameter_spec(
        &self,
        plugin_id: PluginId,
        parameter: usize,
    ) -> Option<ParameterSpec> {
        self.automations
            .read()
            .get(&plugin_id)
            .and_then(|lane| lane.parameter_spec(parameter))
    }
}

pub struct Engine {
    pub graph: crate::sched::graph::Graph,
    pub event_lane: crate::sched::events::EventLane,
    pub sample_pos: u64,
    pub transport: crate::transport::Transport,
    pub pool: crate::sched::executor::RtPool,
    pub parallel_cfg: crate::config::RtParallelCfg,
    max_nodes: usize,
    event_capacity: usize,
}

impl Engine {
    pub fn new(sr: u32, max_block: u32, event_capacity: usize) -> Self {
        let mut graph = crate::sched::graph::Graph::new();
        let pass_id = 0u32;
        let gain_id = 1u32;
        graph
            .nodes
            .push(Box::new(crate::sched::PassThrough::new(pass_id, "input")));
        graph
            .nodes
            .push(Box::new(crate::sched::Gain::new(gain_id, 0)));
        graph.topo = vec![pass_id, gain_id];
        crate::sched::graph::build(&mut graph);
        for node in graph.nodes.iter_mut() {
            node.prepare(sr, max_block);
        }

        let capacity = event_capacity.max(1);
        let parallel_cfg = crate::config::RtParallelCfg::default();
        let (_, worker_cores) = crate::rt::cpu::pick_cores(&parallel_cfg);
        let pool = crate::sched::executor::RtPool::new(
            graph.nodes.len().max(64),
            parallel_cfg.workers as usize,
            &worker_cores,
        );

        let max_nodes = graph.nodes.len();
        Self {
            graph,
            event_lane: crate::sched::events::EventLane::with_capacity(capacity),
            sample_pos: 0,
            transport: crate::transport::Transport::with_sample_rate(sr),
            pool,
            parallel_cfg,
            max_nodes,
            event_capacity: capacity,
        }
    }

    pub fn configure(&mut self, sr: u32, max_block: u32) {
        self.transport.set_sample_rate(sr);
        for node in self.graph.nodes.iter_mut() {
            node.prepare(sr, max_block);
        }
        self.rebuild();
    }

    pub fn reset(&mut self) {
        self.sample_pos = 0;
        self.transport.sample_pos.store(0, Ordering::Relaxed);
        self.event_lane = crate::sched::events::EventLane::with_capacity(self.event_capacity);
    }

    pub fn rebuild(&mut self) {
        crate::sched::graph::build(&mut self.graph);
        if self.graph.nodes.len() > self.max_nodes
            || self.pool.capacity() < self.graph.nodes.len()
            || self.pool_capacity_mismatch()
        {
            let (_, worker_cores) = crate::rt::cpu::pick_cores(&self.parallel_cfg);
            self.pool = crate::sched::executor::RtPool::new(
                self.graph.nodes.len().max(64),
                self.parallel_cfg.workers as usize,
                &worker_cores,
            );
        }
        self.max_nodes = self.graph.nodes.len();
    }

    fn pool_capacity_mismatch(&self) -> bool {
        self.pool.worker_count() != self.parallel_cfg.workers as usize
    }
}
