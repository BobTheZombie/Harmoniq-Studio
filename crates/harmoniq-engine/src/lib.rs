//! Real-time audio engine coordinating audio, MIDI, and plugin processing.

use std::sync::Arc;

use anyhow::Result;
use harmoniq_audioio::{AudioBlock, AudioConfig, AudioIo, AudioStreamHandle, TransportState};
use harmoniq_graph::Graph;
use harmoniq_host::{NullHost, PluginHost};
use harmoniq_project::Project;
use harmoniq_utils::rt::{rt_queue, snap_channel, RtReceiver, RtSender, SnapReceiver, SnapSender};
use parking_lot::Mutex;
use tracing::error;

/// Engine command queue capacity.
const COMMAND_QUEUE: usize = 1024;

/// Representation of the transport state.
#[derive(Debug, Clone)]
pub struct Transport {
    /// Playhead position in samples.
    pub position: u64,
    /// Whether playback is running.
    pub playing: bool,
    /// Current tempo in beats per minute.
    pub bpm: f32,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            position: 0,
            playing: false,
            bpm: 128.0,
        }
    }
}

/// Primary audio engine structure.
pub struct Engine {
    audio: AudioIo,
    host: Arc<Mutex<NullHost>>,
    graph: Arc<Mutex<GraphRuntime>>,
    transport: Arc<Mutex<Transport>>,
    mixer: MixerState,
    rt_cmd_rx: Option<RtReceiver<EngineCmd>>,
    ui_snap_tx: SnapSender<EngineSnapshot>,
    stream: Option<AudioStreamHandle>,
}

impl Engine {
    /// Constructs a new engine along with a handle for the UI thread.
    pub fn new(config: Option<AudioConfig>) -> Result<(Self, EngineHandle)> {
        let audio = AudioIo::new(config)?;
        let host = Arc::new(Mutex::new(NullHost::new()));
        let graph = Arc::new(Mutex::new(GraphRuntime::default()));
        let transport = Arc::new(Mutex::new(Transport::default()));
        let mixer = MixerState::default();
        let (cmd_tx, cmd_rx) = rt_queue(COMMAND_QUEUE);
        let (snap_tx, snap_rx) = snap_channel();
        let engine = Self {
            audio,
            host,
            graph,
            transport,
            mixer,
            rt_cmd_rx: Some(cmd_rx),
            ui_snap_tx: snap_tx.clone(),
            stream: None,
        };
        let handle = EngineHandle {
            commands: Mutex::new(cmd_tx),
            snapshots: snap_rx,
            snapshot_tx: snap_tx,
        };
        Ok((engine, handle))
    }

    /// Starts the audio thread and begins processing.
    pub fn start(&mut self) -> Result<()> {
        let mut rt_cmd_rx = self.rt_cmd_rx.take().expect("engine already started");
        let host = self.host.clone();
        let graph = self.graph.clone();
        let transport = self.transport.clone();
        let mixer = self.mixer.clone();
        let snap_tx = self.ui_snap_tx.clone();
        let config = self.audio.config().clone();
        host.lock().set_sample_rate(config.sample_rate as f32);
        host.lock().set_block_size(config.block_size);
        let stream = self.audio.start_stream(move |block| {
            while let Some(cmd) = rt_cmd_rx.try_recv() {
                apply_command(&graph, &mixer, &transport, cmd);
            }
            process_block(block, &graph, &mixer, &transport);
            publish_snapshot(&snap_tx, &mixer, &transport);
        })?;
        self.stream = Some(stream);
        Ok(())
    }
}

/// Handle used by the UI to interact with the engine.
pub struct EngineHandle {
    commands: Mutex<RtSender<EngineCmd>>,
    snapshots: SnapReceiver<EngineSnapshot>,
    snapshot_tx: SnapSender<EngineSnapshot>,
}

impl EngineHandle {
    /// Sends a command to the engine thread.
    pub fn submit(&self, cmd: EngineCmd) {
        if let Err(cmd) = self.commands.lock().try_send(cmd) {
            error!(?cmd, "engine command queue overflow");
        }
    }

    /// Retrieves the latest snapshot.
    pub fn latest_snapshot(&self) -> Option<EngineSnapshot> {
        self.snapshots.recv_latest()
    }

    /// Injects a snapshot directly (used for testing).
    pub fn push_snapshot(&self, snapshot: EngineSnapshot) {
        self.snapshot_tx.send(snapshot);
    }
}

/// Commands accepted by the engine.
#[derive(Debug, Clone)]
pub enum EngineCmd {
    /// Toggle transport playback.
    TogglePlay,
    /// Stop playback and reset the playhead.
    Stop,
    /// Seek to a specific sample position.
    Seek(u64),
    /// Adjust the global tempo.
    SetTempo(f32),
    /// Update the gain of a mixer track.
    SetTrackGain { track: usize, gain: f32 },
    /// Update the pan of a mixer track.
    SetTrackPan { track: usize, pan: f32 },
    /// Mute or unmute a track.
    SetTrackMute { track: usize, mute: bool },
    /// Replace the current routing graph.
    LoadGraph(Graph),
    /// Replace the engine state with a project snapshot.
    LoadProject(Project),
}

/// Snapshot communicated to the UI thread.
#[derive(Debug, Clone)]
pub struct EngineSnapshot {
    /// Transport state.
    pub transport: TransportState,
    /// Mixer channel snapshots.
    pub mixer: Vec<MixerChannelSnapshot>,
}

/// Runtime graph prepared for rendering.
#[derive(Debug, Clone, Default)]
pub struct GraphRuntime {
    graph: Graph,
    order: Vec<u32>,
}

impl GraphRuntime {
    fn update(&mut self, mut graph: Graph) {
        graph.recompute_pdc();
        self.order = graph
            .topological_order()
            .into_iter()
            .map(|id| id.0)
            .collect();
        self.graph = graph;
    }
}

/// Mixer state shared between threads.
#[derive(Debug, Clone)]
pub struct MixerState {
    inner: Arc<Mutex<Vec<MixerChannel>>>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(
                (0..24)
                    .map(|index| MixerChannel {
                        name: format!("Track {index}"),
                        gain: 1.0,
                        pan: 0.0,
                        mute: false,
                        latency: 0,
                        peak: 0.0,
                        rms: 0.0,
                    })
                    .collect(),
            )),
        }
    }
}

impl MixerState {
    fn with_channel<F>(&self, index: usize, f: F)
    where
        F: FnOnce(&mut MixerChannel),
    {
        let mut channels = self.inner.lock();
        if let Some(channel) = channels.get_mut(index) {
            f(channel);
        }
    }

    fn snapshot(&self) -> Vec<MixerChannelSnapshot> {
        self.inner
            .lock()
            .iter()
            .map(|channel| MixerChannelSnapshot {
                name: channel.name.clone(),
                gain: channel.gain,
                pan: channel.pan,
                mute: channel.mute,
                latency: channel.latency,
                peak: channel.peak,
                rms: channel.rms,
            })
            .collect()
    }
}

/// Individual mixer channel state.
#[derive(Debug, Clone)]
pub struct MixerChannel {
    name: String,
    gain: f32,
    pan: f32,
    mute: bool,
    latency: u32,
    peak: f32,
    rms: f32,
}

/// Snapshot of a mixer channel consumed by the UI.
#[derive(Debug, Clone)]
pub struct MixerChannelSnapshot {
    /// Channel name.
    pub name: String,
    /// Linear gain value.
    pub gain: f32,
    /// Pan from -1.0 to 1.0.
    pub pan: f32,
    /// Whether the channel is muted.
    pub mute: bool,
    /// Latency in samples.
    pub latency: u32,
    /// Peak meter value.
    pub peak: f32,
    /// RMS meter value.
    pub rms: f32,
}

fn apply_command(
    graph: &Arc<Mutex<GraphRuntime>>,
    mixer: &MixerState,
    transport: &Arc<Mutex<Transport>>,
    cmd: EngineCmd,
) {
    match cmd {
        EngineCmd::TogglePlay => {
            let mut transport = transport.lock();
            transport.playing = !transport.playing;
        }
        EngineCmd::Stop => {
            let mut transport = transport.lock();
            transport.playing = false;
            transport.position = 0;
        }
        EngineCmd::Seek(position) => {
            transport.lock().position = position;
        }
        EngineCmd::SetTempo(bpm) => {
            transport.lock().bpm = bpm;
        }
        EngineCmd::SetTrackGain { track, gain } => {
            mixer.with_channel(track, |channel| channel.gain = gain)
        }
        EngineCmd::SetTrackPan { track, pan } => {
            mixer.with_channel(track, |channel| channel.pan = pan)
        }
        EngineCmd::SetTrackMute { track, mute } => {
            mixer.with_channel(track, |channel| channel.mute = mute)
        }
        EngineCmd::LoadGraph(new_graph) => {
            graph.lock().update(new_graph);
        }
        EngineCmd::LoadProject(project) => {
            graph.lock().update(project.graph);
            let mut channels = mixer.inner.lock();
            for (channel, track) in channels.iter_mut().zip(project.tracks.iter()) {
                channel.name = track.name.clone();
            }
        }
    }
}

fn process_block(
    block: &mut AudioBlock,
    _graph: &Arc<Mutex<GraphRuntime>>,
    mixer: &MixerState,
    transport: &Arc<Mutex<Transport>>,
) {
    block.clear();
    let mut transport = transport.lock();
    if transport.playing {
        transport.position += block.frames as u64;
    }
    drop(transport);
    let mut channels = mixer.inner.lock();
    for (index, channel) in channels.iter_mut().enumerate() {
        let data = block.channels.get(index);
        let mut peak: f32 = 0.0;
        let mut sum = 0.0;
        if let Some(samples) = data {
            for sample in samples {
                let abs = sample.abs();
                peak = peak.max(abs);
                sum += sample * sample;
            }
            if !samples.is_empty() {
                channel.rms = (sum / samples.len() as f32).sqrt();
            } else {
                channel.rms = 0.0;
            }
        } else {
            channel.rms = 0.0;
        }
        channel.peak = peak;
    }
}

fn publish_snapshot(
    snap_tx: &SnapSender<EngineSnapshot>,
    mixer: &MixerState,
    transport: &Arc<Mutex<Transport>>,
) {
    let transport_state = {
        let transport = transport.lock();
        TransportState {
            tempo: harmoniq_utils::time::TempoInfo {
                bpm: transport.bpm,
                time_signature_numerator: 4,
                time_signature_denominator: 4,
            },
            position: transport.position,
            playing: transport.playing,
        }
    };
    let snapshot = EngineSnapshot {
        transport: transport_state,
        mixer: mixer.snapshot(),
    };
    snap_tx.send(snapshot);
}

/// Renders a number of silent blocks for testing.
pub fn render_offline(graph: Graph, blocks: usize) -> Vec<AudioBlock> {
    let (engine, _) = Engine::new(Some(AudioConfig::default())).expect("engine");
    engine.graph.lock().update(graph);
    let mut outputs = Vec::new();
    for _ in 0..blocks {
        let mut block = AudioBlock::new(
            engine.audio.config().channels,
            engine.audio.config().block_size,
        );
        process_block(&mut block, &engine.graph, &engine.mixer, &engine.transport);
        outputs.push(block);
    }
    outputs
}
