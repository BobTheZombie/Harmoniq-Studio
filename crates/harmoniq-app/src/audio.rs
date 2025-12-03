use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Once};
use std::thread::{self, JoinHandle};

#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use std::path::Path;

use anyhow::{anyhow, Context};
use clap::builder::PossibleValue;
use clap::ValueEnum;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};
use crossbeam_queue::ArrayQueue;
#[cfg(target_os = "linux")]
use harmoniq_engine::sound_server::alsa_devices_available;
#[cfg(all(target_os = "linux", feature = "openasio"))]
use harmoniq_engine::sound_server::UltraOpenAsioOptions;
use harmoniq_engine::{
    AudioBuffer, AudioClip, BufferConfig, ChannelLayout, EngineCommandQueue, HarmoniqEngine,
};
use parking_lot::Mutex;
use tracing::{info, warn};

#[cfg(target_os = "linux")]
use linux_asio::LinuxAsioDriver;

use crate::midi::{open_midi_input, MidiConnection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBackend {
    Auto,
    Alsa,
    Harmoniq,
    #[cfg(feature = "openasio")]
    OpenAsio,
    PulseAudio,
    PipeWire,
    Asio,
    Asio4All,
    Wasapi,
    CoreAudio,
}

struct RealtimeEngineFacade {
    state: Arc<AudioThreadState>,
    render_thread: Option<JoinHandle<()>>,
}

impl RealtimeEngineFacade {
    fn new(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        channels: usize,
    ) -> anyhow::Result<Self> {
        let state = Arc::new(AudioThreadState::new(config.block_size, channels));
        let render_state = Arc::clone(&state);
        let render_engine = Arc::clone(&engine);
        let render_config = config.clone();
        let render_thread = thread::Builder::new()
            .name("harmoniq-cpal-render".into())
            .spawn(move || {
                run_engine_render_loop(render_engine, render_config, render_state);
            })
            .context("failed to spawn realtime engine render thread")?;

        Ok(Self {
            state,
            render_thread: Some(render_thread),
        })
    }

    fn callback_handle(&mut self) -> AudioCallbackHandle {
        AudioCallbackHandle::new(Arc::clone(&self.state))
    }

    fn render_output<T>(&self, handle: &mut AudioCallbackHandle, output: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        handle.render_into(output);
    }
}

impl Drop for RealtimeEngineFacade {
    fn drop(&mut self) {
        self.state.running.store(false, AtomicOrdering::Relaxed);
        if let Some(handle) = self.render_thread.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Debug, Clone)]
enum AudioThreadNotification {
    EngineError(String),
}

struct AudioThreadState {
    buffers: [UnsafeCell<Vec<f32>>; 2],
    frames_per_buffer: usize,
    channels: usize,
    ready_index: AtomicUsize,
    ready_frames: AtomicUsize,
    running: AtomicBool,
    notifications: Arc<ArrayQueue<AudioThreadNotification>>,
}

unsafe impl Sync for AudioThreadState {}

impl AudioThreadState {
    fn new(frames_per_buffer: usize, channels: usize) -> Self {
        let frames = frames_per_buffer.max(1);
        let channel_count = channels.max(1);
        let samples = frames.saturating_mul(channel_count);
        Self {
            buffers: [
                UnsafeCell::new(vec![0.0; samples]),
                UnsafeCell::new(vec![0.0; samples]),
            ],
            frames_per_buffer: frames,
            channels: channel_count,
            ready_index: AtomicUsize::new(0),
            ready_frames: AtomicUsize::new(frames),
            running: AtomicBool::new(true),
            notifications: Arc::new(ArrayQueue::new(32)),
        }
    }
}

struct AudioCallbackHandle {
    state: Arc<AudioThreadState>,
    active_buffer: usize,
    cursor: usize,
}

impl AudioCallbackHandle {
    fn new(state: Arc<AudioThreadState>) -> Self {
        let active_buffer = state.ready_index.load(AtomicOrdering::Acquire).min(1);
        Self {
            state,
            active_buffer,
            cursor: 0,
        }
    }

    fn render_into<T>(&mut self, output: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        RT_DENORM_INIT.call_once(|| harmoniq_engine::rt::enable_denorm_mode());

        let channels = self.state.channels;
        if channels == 0 {
            return;
        }

        let mut offset = 0usize;
        let silence = T::from_sample(0.0);
        while offset < output.len() {
            let ready_index = self.state.ready_index.load(AtomicOrdering::Acquire);
            let available_frames = self.state.ready_frames.load(AtomicOrdering::Acquire);
            if ready_index != self.active_buffer
                && self.cursor >= available_frames.saturating_mul(channels)
            {
                self.active_buffer = ready_index;
                self.cursor = 0;
            }

            let available_samples = available_frames
                .saturating_mul(channels)
                .saturating_sub(self.cursor);
            if available_samples == 0 {
                for sample in &mut output[offset..] {
                    *sample = silence;
                }
                break;
            }

            let samples_to_copy = (output.len() - offset).min(available_samples);
            let buffer = unsafe { &*self.state.buffers[self.active_buffer].get() };
            let end = self.cursor.saturating_add(samples_to_copy);
            for (dst, src) in output[offset..offset + samples_to_copy]
                .iter_mut()
                .zip(&buffer[self.cursor..end])
            {
                *dst = T::from_sample(*src);
            }

            self.cursor = end;
            offset += samples_to_copy;

            if self.cursor >= available_frames.saturating_mul(channels) {
                let next_ready = self.state.ready_index.load(AtomicOrdering::Acquire);
                if next_ready != self.active_buffer {
                    self.active_buffer = next_ready;
                    self.cursor = 0;
                }
            }
        }
    }
}

fn run_engine_render_loop(
    engine: Arc<Mutex<HarmoniqEngine>>,
    config: BufferConfig,
    state: Arc<AudioThreadState>,
) {
    let mut buffer = AudioBuffer::from_config(&config);
    let mut interleaved = vec![0.0f32; state.frames_per_buffer.saturating_mul(state.channels)];
    refill_audio_buffer(
        &engine,
        &mut buffer,
        &mut interleaved,
        state.channels,
        &state,
    );

    while state.running.load(AtomicOrdering::Relaxed) {
        refill_audio_buffer(
            &engine,
            &mut buffer,
            &mut interleaved,
            state.channels,
            &state,
        );
    }
}

fn refill_audio_buffer(
    engine: &Arc<Mutex<HarmoniqEngine>>,
    buffer: &mut AudioBuffer,
    interleaved: &mut [f32],
    channels: usize,
    state: &Arc<AudioThreadState>,
) {
    buffer.clear();
    let process_result = {
        let mut guard = engine.lock();
        guard.process_block(buffer)
    };

    let mut frames = buffer.len().min(state.frames_per_buffer);
    if let Err(err) = process_result {
        frames = 0;
        push_rt_notice(
            &state.notifications,
            AudioThreadNotification::EngineError(err.to_string()),
        );
    }

    let samples_to_copy = frames.saturating_mul(channels);
    interleave_into(interleaved, buffer, channels, samples_to_copy);

    let target_buffer = state.ready_index.load(AtomicOrdering::Acquire) ^ 1;
    if target_buffer < state.buffers.len() {
        let dest = unsafe { &mut *state.buffers[target_buffer].get() };
        dest[..samples_to_copy].copy_from_slice(&interleaved[..samples_to_copy]);
        for sample in &mut dest[samples_to_copy..] {
            *sample = 0.0;
        }
    }

    state
        .ready_frames
        .store(frames.min(state.frames_per_buffer), AtomicOrdering::Release);
    state
        .ready_index
        .store(target_buffer.min(1), AtomicOrdering::Release);
}

fn interleave_into(target: &mut [f32], buffer: &AudioBuffer, channels: usize, samples: usize) {
    let frames = buffer.len();
    let channel_count = buffer.channel_count();
    let mut index = 0usize;
    let max_samples = target.len().min(samples);
    for frame in 0..frames {
        for channel in 0..channels {
            if index >= max_samples {
                return;
            }
            let value = if channel < channel_count {
                buffer.channel(channel)[frame]
            } else {
                0.0
            };
            target[index] = value;
            index += 1;
        }
    }

    for slot in &mut target[index..max_samples] {
        *slot = 0.0;
    }
}

fn push_rt_notice(
    queue: &Arc<ArrayQueue<AudioThreadNotification>>,
    notice: AudioThreadNotification,
) {
    if let Err(notice) = queue.push(notice) {
        let _ = queue.force_push(notice);
    }
}

impl ValueEnum for AudioBackend {
    fn value_variants<'a>() -> &'a [Self] {
        #[cfg(feature = "openasio")]
        {
            const VARIANTS: &[AudioBackend] = &[
                AudioBackend::Auto,
                AudioBackend::Alsa,
                AudioBackend::Harmoniq,
                AudioBackend::OpenAsio,
                AudioBackend::PulseAudio,
                AudioBackend::PipeWire,
                AudioBackend::Asio,
                AudioBackend::Asio4All,
                AudioBackend::Wasapi,
                AudioBackend::CoreAudio,
            ];
            VARIANTS
        }

        #[cfg(not(feature = "openasio"))]
        {
            const VARIANTS: &[AudioBackend] = &[
                AudioBackend::Auto,
                AudioBackend::Alsa,
                AudioBackend::Harmoniq,
                AudioBackend::PulseAudio,
                AudioBackend::PipeWire,
                AudioBackend::Asio,
                AudioBackend::Asio4All,
                AudioBackend::Wasapi,
                AudioBackend::CoreAudio,
            ];
            VARIANTS
        }
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        let value = match self {
            AudioBackend::Auto => PossibleValue::new("auto"),
            AudioBackend::Alsa => PossibleValue::new("alsa"),
            AudioBackend::Harmoniq => PossibleValue::new("harmoniq"),
            #[cfg(feature = "openasio")]
            AudioBackend::OpenAsio => PossibleValue::new("open-asio"),
            AudioBackend::PulseAudio => PossibleValue::new("pulse-audio"),
            AudioBackend::PipeWire => PossibleValue::new("pipe-wire"),
            AudioBackend::Asio => PossibleValue::new("asio"),
            AudioBackend::Asio4All => PossibleValue::new("asio4-all"),
            AudioBackend::Wasapi => PossibleValue::new("wasapi"),
            AudioBackend::CoreAudio => PossibleValue::new("core-audio"),
        };

        Some(value)
    }

    fn from_str(input: &str, _ignore_case: bool) -> Result<Self, String> {
        let normalized = input.replace('_', "-");
        let normalized = normalized.trim();

        if normalized.eq_ignore_ascii_case("auto") {
            return Ok(AudioBackend::Auto);
        }

        if normalized.eq_ignore_ascii_case("alsa") {
            return Ok(AudioBackend::Alsa);
        }

        if normalized.eq_ignore_ascii_case("harmoniq") {
            return Ok(AudioBackend::Harmoniq);
        }

        #[cfg(feature = "openasio")]
        if normalized.eq_ignore_ascii_case("open-asio")
            || normalized.eq_ignore_ascii_case("openasio")
        {
            return Ok(AudioBackend::OpenAsio);
        }

        if normalized.eq_ignore_ascii_case("pulse-audio")
            || normalized.eq_ignore_ascii_case("pulseaudio")
        {
            return Ok(AudioBackend::PulseAudio);
        }

        if normalized.eq_ignore_ascii_case("pipe-wire")
            || normalized.eq_ignore_ascii_case("pipewire")
        {
            return Ok(AudioBackend::PipeWire);
        }

        if normalized.eq_ignore_ascii_case("asio") {
            return Ok(AudioBackend::Asio);
        }

        if normalized.eq_ignore_ascii_case("asio4-all")
            || normalized.eq_ignore_ascii_case("asio4all")
        {
            return Ok(AudioBackend::Asio4All);
        }

        if normalized.eq_ignore_ascii_case("wasapi") {
            return Ok(AudioBackend::Wasapi);
        }

        if normalized.eq_ignore_ascii_case("core-audio")
            || normalized.eq_ignore_ascii_case("coreaudio")
        {
            return Ok(AudioBackend::CoreAudio);
        }

        Err(input.to_owned())
    }
}

static RT_DENORM_INIT: Once = Once::new();

impl AudioBackend {
    pub fn host_id(self) -> Option<cpal::HostId> {
        match self {
            AudioBackend::Auto => None,
            AudioBackend::Alsa => {
                #[cfg(target_os = "linux")]
                {
                    Some(cpal::HostId::Alsa)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    None
                }
            }
            AudioBackend::Harmoniq => None,
            #[cfg(feature = "openasio")]
            AudioBackend::OpenAsio => None,
            AudioBackend::PulseAudio => {
                #[cfg(target_os = "linux")]
                {
                    Some(cpal::HostId::Alsa)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    None
                }
            }
            AudioBackend::PipeWire => {
                #[cfg(target_os = "linux")]
                {
                    Some(cpal::HostId::Alsa)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    None
                }
            }
            AudioBackend::Asio | AudioBackend::Asio4All => {
                #[cfg(target_os = "windows")]
                {
                    Some(cpal::HostId::Asio)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    None
                }
            }
            AudioBackend::Wasapi => {
                #[cfg(target_os = "windows")]
                {
                    Some(cpal::HostId::Wasapi)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    None
                }
            }
            AudioBackend::CoreAudio => {
                #[cfg(target_os = "macos")]
                {
                    Some(cpal::HostId::CoreAudio)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    None
                }
            }
        }
    }

    #[allow(unreachable_patterns)]
    pub fn from_host_id(id: cpal::HostId) -> Option<Self> {
        match id {
            #[cfg(target_os = "linux")]
            cpal::HostId::Alsa => {
                if is_pipewire_active() {
                    Some(Self::PipeWire)
                } else if is_pulseaudio_active() {
                    Some(Self::PulseAudio)
                } else {
                    Some(Self::Alsa)
                }
            }
            #[cfg(target_os = "windows")]
            cpal::HostId::Asio => Some(Self::Asio),
            #[cfg(target_os = "windows")]
            cpal::HostId::Wasapi => Some(Self::Wasapi),
            #[cfg(target_os = "macos")]
            cpal::HostId::CoreAudio => Some(Self::CoreAudio),
            _ => None,
        }
    }
}

impl fmt::Display for AudioBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AudioBackend::Auto => "auto",
            AudioBackend::Alsa => "ALSA",
            AudioBackend::Harmoniq => "Harmoniq Ultra",
            #[cfg(feature = "openasio")]
            AudioBackend::OpenAsio => "OpenASIO",
            AudioBackend::PulseAudio => "PulseAudio",
            AudioBackend::PipeWire => "PipeWire",
            AudioBackend::Asio => "ASIO",
            AudioBackend::Asio4All => "ASIO4ALL",
            AudioBackend::Wasapi => "WASAPI",
            AudioBackend::CoreAudio => "CoreAudio",
        };
        write!(f, "{name}")
    }
}

#[cfg(target_os = "linux")]
fn is_pipewire_active() -> bool {
    env::var_os("PIPEWIRE_RUNTIME_DIR").is_some()
        || env::var_os("PIPEWIRE_LATENCY").is_some()
        || env::var_os("PIPEWIRE_VERSION").is_some()
        || env::var_os("PIPEWIRE_CONFIG_DIR").is_some()
        || env::var_os("PIPEWIRE_CONFIG_PATH").is_some()
        || env::var_os("WIREPLUMBER_CONFIG_DIR").is_some()
        || env::var_os("WIREPLUMBER_CONFIG_FILE").is_some()
        || env::var("XDG_RUNTIME_DIR")
            .ok()
            .map(|dir| Path::new(&dir).join("pipewire-0").exists())
            .unwrap_or(false)
        || Path::new("/run/pipewire-0").exists()
        || Path::new("/run/pipewire").exists()
}

#[cfg(target_os = "linux")]
fn is_pulseaudio_active() -> bool {
    env::var_os("PULSE_SERVER").is_some()
        || env::var_os("PULSE_RUNTIME_PATH").is_some()
        || env::var_os("PULSE_STATE_PATH").is_some()
        || env::var("XDG_RUNTIME_DIR")
            .ok()
            .map(|dir| Path::new(&dir).join("pulse/native").exists())
            .unwrap_or(false)
        || Path::new("/run/pulse/native").exists()
}

#[cfg(target_os = "linux")]
fn linux_host_prefix(host: cpal::HostId) -> String {
    #[allow(unreachable_patterns)]
    match host {
        cpal::HostId::Alsa => "alsa".to_string(),
        other => other.name().to_ascii_lowercase(),
    }
}

#[cfg(target_os = "linux")]
fn linux_device_identifier(host: cpal::HostId, name: &str) -> String {
    format!("{}::{name}", linux_host_prefix(host))
}

#[cfg(target_os = "linux")]
fn parse_linux_device_selector(selector: &str) -> Option<(cpal::HostId, &str)> {
    let (prefix, rest) = selector.split_once("::")?;
    let host_id = match prefix {
        "alsa" | "pipewire" | "pulseaudio" => cpal::HostId::Alsa,
        other
            if other.eq_ignore_ascii_case("alsa")
                || other.eq_ignore_ascii_case("pipewire")
                || other.eq_ignore_ascii_case("pulseaudio") =>
        {
            cpal::HostId::Alsa
        }
        _ => return None,
    };
    Some((host_id, rest))
}

#[cfg(target_os = "linux")]
fn sanitize_device_for_backend(backend: AudioBackend, selection: Option<&str>) -> Option<String> {
    let host_id = match backend {
        AudioBackend::PulseAudio | AudioBackend::PipeWire | AudioBackend::Alsa => {
            Some(cpal::HostId::Alsa)
        }
        _ => None,
    }?;

    selection.and_then(|selector| {
        if let Some((host, name)) = parse_linux_device_selector(selector) {
            if host == host_id {
                Some(linux_device_identifier(host, name))
            } else {
                None
            }
        } else if selector.is_empty() {
            None
        } else {
            Some(linux_device_identifier(host_id, selector))
        }
    })
}

#[cfg(target_os = "linux")]
fn sanitize_asio_selector(selection: Option<&str>) -> (Option<cpal::HostId>, Option<String>) {
    match selection {
        Some(selector) if !selector.is_empty() => {
            if let Some((host, name)) = parse_linux_device_selector(selector) {
                (Some(host), Some(linux_device_identifier(host, name)))
            } else {
                let host = select_default_linux_asio_host();
                host.map(|host| (Some(host), Some(linux_device_identifier(host, selector))))
                    .unwrap_or((None, None))
            }
        }
        _ => (select_default_linux_asio_host(), None),
    }
}

#[cfg(target_os = "linux")]
fn select_default_linux_asio_host() -> Option<cpal::HostId> {
    if !alsa_devices_available() {
        return None;
    }
    let available = cpal::available_hosts();
    if available.contains(&cpal::HostId::Alsa) {
        Some(cpal::HostId::Alsa)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_backend_label(backend: AudioBackend, _host_id: Option<cpal::HostId>) -> String {
    match backend {
        AudioBackend::PipeWire => "PipeWire (via ALSA)".to_string(),
        AudioBackend::PulseAudio => "PulseAudio (via ALSA)".to_string(),
        AudioBackend::Alsa => "ALSA".to_string(),
        AudioBackend::Harmoniq => "Harmoniq Ultra".to_string(),
        #[cfg(feature = "openasio")]
        AudioBackend::OpenAsio => "OpenASIO".to_string(),
        AudioBackend::Asio => "Linux ASIO (ALSA)".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct AudioRuntimeOptions {
    pub backend: AudioBackend,
    pub midi_input: Option<String>,
    pub enable_audio: bool,
    pub output_device: Option<String>,
    #[cfg(feature = "openasio")]
    pub openasio_driver: Option<String>,
    #[cfg(feature = "openasio")]
    pub openasio_device: Option<String>,
    #[cfg(feature = "openasio")]
    pub openasio_sample_rate: Option<u32>,
    #[cfg(feature = "openasio")]
    pub openasio_buffer_frames: Option<u32>,
}

impl AudioRuntimeOptions {
    pub fn new(backend: AudioBackend, midi_input: Option<String>, enable_audio: bool) -> Self {
        Self {
            backend,
            midi_input,
            enable_audio,
            output_device: None,
            #[cfg(feature = "openasio")]
            openasio_driver: None,
            #[cfg(feature = "openasio")]
            openasio_device: None,
            #[cfg(feature = "openasio")]
            openasio_sample_rate: None,
            #[cfg(feature = "openasio")]
            openasio_buffer_frames: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enable_audio
    }

    pub fn backend(&self) -> AudioBackend {
        self.backend
    }

    pub fn output_device(&self) -> Option<&str> {
        self.output_device.as_deref()
    }

    pub fn set_backend(&mut self, backend: AudioBackend) {
        self.backend = backend;
    }

    pub fn set_output_device(&mut self, output_device: Option<String>) {
        self.output_device = output_device;
    }

    pub fn with_output_device(mut self, output_device: Option<String>) -> Self {
        self.output_device = output_device;
        self
    }
}

pub struct RealtimeAudio {
    #[cfg(target_os = "linux")]
    _stream: StreamBackend,
    #[cfg(not(target_os = "linux"))]
    _stream: cpal::Stream,
    _midi: Option<MidiConnection>,
    backend: AudioBackend,
    device_name: String,
    device_id: Option<String>,
    host_label: Option<String>,
}

#[cfg(target_os = "linux")]
enum StreamBackend {
    Cpal(cpal::Stream),
    LinuxAsio(LinuxAsioDriver),
    Ultra(harmoniq_engine::sound_server::UltraLowLatencyServer),
    #[cfg(feature = "openasio")]
    OpenAsio(openasio_rt::OpenAsioHandle),
}

struct StreamCreation {
    #[cfg(target_os = "linux")]
    stream: StreamBackend,
    #[cfg(not(target_os = "linux"))]
    stream: cpal::Stream,
    backend: AudioBackend,
    device_name: String,
    device_id: Option<String>,
    host_label: Option<String>,
    sample_rate: u32,
}

impl RealtimeAudio {
    #[cfg(target_os = "linux")]
    fn push_unique_backend(candidates: &mut Vec<AudioBackend>, backend: AudioBackend) {
        if !candidates.contains(&backend) {
            candidates.push(backend);
        }
    }

    pub fn start(
        engine: Arc<Mutex<HarmoniqEngine>>,
        command_queue: EngineCommandQueue,
        config: BufferConfig,
        options: AudioRuntimeOptions,
    ) -> anyhow::Result<Self> {
        if !options.enable_audio {
            anyhow::bail!("audio output disabled");
        }

        let stream_info = Self::initialise_stream(Arc::clone(&engine), config.clone(), &options)?;
        let midi = open_midi_input(options.midi_input.clone(), command_queue)
            .context("failed to initialise MIDI input")?;

        #[cfg(target_os = "linux")]
        let StreamCreation {
            stream,
            backend,
            device_name,
            device_id,
            host_label,
            sample_rate,
        } = stream_info;
        #[cfg(not(target_os = "linux"))]
        let StreamCreation {
            stream,
            backend,
            device_name,
            device_id,
            host_label,
            sample_rate,
        } = stream_info;

        if let Some(ref host) = host_label {
            info!(backend = %backend, host = %host, device = %device_name, "Started realtime audio");
        } else {
            info!(backend = %backend, device = %device_name, "Started realtime audio");
        }

        let (transport_handle, playing_state) = {
            let engine_guard = engine.lock();
            (
                engine_guard.transport_metrics(),
                matches!(
                    engine_guard.transport(),
                    harmoniq_engine::TransportState::Playing
                        | harmoniq_engine::TransportState::Recording
                ),
            )
        };
        transport_handle
            .sr
            .store(sample_rate.max(1) as u32, AtomicOrdering::Relaxed);
        transport_handle
            .sample_pos
            .store(0, AtomicOrdering::Relaxed);
        transport_handle
            .playing
            .store(playing_state, AtomicOrdering::Relaxed);

        Ok(Self {
            #[cfg(target_os = "linux")]
            _stream: stream,
            #[cfg(not(target_os = "linux"))]
            _stream: stream,
            _midi: midi,
            backend,
            device_name,
            device_id,
            host_label,
        })
    }

    fn initialise_stream(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        options: &AudioRuntimeOptions,
    ) -> anyhow::Result<StreamCreation> {
        if matches!(options.backend, AudioBackend::Auto) {
            #[cfg(target_os = "linux")]
            {
                let mut candidates: Vec<AudioBackend> = Vec::new();
                let available = cpal::available_hosts();
                let has_alsa = available.contains(&cpal::HostId::Alsa);
                let has_native_audio = alsa_devices_available();

                if has_native_audio {
                    Self::push_unique_backend(&mut candidates, AudioBackend::Harmoniq);
                }

                if has_native_audio && has_alsa && is_pulseaudio_active() {
                    Self::push_unique_backend(&mut candidates, AudioBackend::PulseAudio);
                }
                if has_native_audio && has_alsa && is_pipewire_active() {
                    Self::push_unique_backend(&mut candidates, AudioBackend::PipeWire);
                }
                if has_native_audio && has_alsa {
                    Self::push_unique_backend(&mut candidates, AudioBackend::Alsa);
                }
                if has_native_audio && select_default_linux_asio_host().is_some() {
                    Self::push_unique_backend(&mut candidates, AudioBackend::Asio);
                }
                if has_native_audio && has_alsa {
                    Self::push_unique_backend(&mut candidates, AudioBackend::PulseAudio);
                    Self::push_unique_backend(&mut candidates, AudioBackend::PipeWire);
                }
                if candidates.is_empty() {
                    Self::push_unique_backend(&mut candidates, AudioBackend::Auto);
                }

                let mut last_err: Option<anyhow::Error> = None;
                for backend in candidates {
                    match Self::start_with_backend(
                        Arc::clone(&engine),
                        config.clone(),
                        options,
                        backend,
                    ) {
                        Ok(stream) => return Ok(stream),
                        Err(err) => last_err = Some(err),
                    }
                }

                Err(last_err.unwrap_or_else(|| anyhow!("no audio backend available")))
            }
            #[cfg(not(target_os = "linux"))]
            {
                Self::start_with_backend(engine, config, options, AudioBackend::Auto)
            }
        } else {
            Self::start_with_backend(engine, config, options, options.backend)
        }
    }

    #[cfg(target_os = "linux")]
    fn start_with_backend(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        options: &AudioRuntimeOptions,
        backend: AudioBackend,
    ) -> anyhow::Result<StreamCreation> {
        let engine_rate = config.sample_rate.round() as u32;
        match backend {
            #[cfg(feature = "openasio")]
            AudioBackend::OpenAsio => {
                let handle = openasio_rt::OpenAsioHandle::start(
                    Arc::clone(&engine),
                    config.clone(),
                    options,
                )?;
                let device_name = handle.device_name().to_string();
                let device_id = handle.device_id().map(|id| id.to_string());
                Ok(StreamCreation {
                    stream: StreamBackend::OpenAsio(handle),
                    backend: AudioBackend::OpenAsio,
                    device_name,
                    device_id,
                    host_label: Some(linux_backend_label(AudioBackend::OpenAsio, None)),
                    sample_rate: engine_rate,
                })
            }
            AudioBackend::Asio => {
                if !alsa_devices_available() {
                    anyhow::bail!("no ALSA-compatible audio devices detected");
                }
                let (host_hint, selector) =
                    sanitize_asio_selector(options.output_device.as_deref());
                let driver = LinuxAsioDriver::start(
                    Arc::clone(&engine),
                    config,
                    host_hint,
                    selector.as_deref(),
                )
                .context("failed to start Linux ASIO driver")?;
                let device_name = driver.device_name().to_string();
                let device_id = Some(driver.device_id().to_string());
                let host_label = Some(linux_backend_label(
                    AudioBackend::Asio,
                    Some(driver.host_id()),
                ));
                Ok(StreamCreation {
                    stream: StreamBackend::LinuxAsio(driver),
                    backend: AudioBackend::Asio,
                    device_name,
                    device_id,
                    host_label,
                    sample_rate: engine_rate,
                })
            }
            AudioBackend::Harmoniq => {
                if !alsa_devices_available() {
                    anyhow::bail!("no ALSA-compatible audio devices detected");
                }
                let device = options
                    .output_device
                    .clone()
                    .or_else(|| Some("default".to_string()));
                #[allow(unused_mut)]
                let mut base_options =
                    harmoniq_engine::sound_server::UltraLowLatencyOptions::default()
                        .with_device(device.clone());
                #[cfg(feature = "openasio")]
                {
                    base_options = base_options.with_buffer_frames(options.openasio_buffer_frames);
                }

                #[cfg(feature = "openasio")]
                let openasio_request = {
                    let wants_openasio = options.openasio_driver.is_some()
                        || options.openasio_device.is_some()
                        || options.openasio_sample_rate.is_some()
                        || options.openasio_buffer_frames.is_some();
                    if wants_openasio {
                        let default_driver = if cfg!(debug_assertions) {
                            "target/debug/libopenasio_driver_cpal.so"
                        } else {
                            "target/release/libopenasio_driver_cpal.so"
                        };
                        let driver_path = options
                            .openasio_driver
                            .clone()
                            .unwrap_or_else(|| default_driver.to_string());
                        let device_selection = options
                            .openasio_device
                            .clone()
                            .or_else(|| device.clone())
                            .filter(|name| !name.is_empty());
                        Some(
                            UltraOpenAsioOptions::new(driver_path)
                                .with_device(device_selection)
                                .with_sample_rate(options.openasio_sample_rate),
                        )
                    } else {
                        None
                    }
                };

                #[cfg(feature = "openasio")]
                let server = {
                    if let Some(openasio_opts) = openasio_request {
                        match harmoniq_engine::sound_server::UltraLowLatencyServer::start(
                            Arc::clone(&engine),
                            config.clone(),
                            base_options.clone().with_openasio(Some(openasio_opts)),
                        ) {
                            Ok(server) => server,
                            Err(err) => {
                                warn!(?err, "failed to start Harmoniq Ultra with OpenASIO; falling back to ALSA backend");
                                harmoniq_engine::sound_server::UltraLowLatencyServer::start(
                                    Arc::clone(&engine),
                                    config.clone(),
                                    base_options.clone(),
                                )?
                            }
                        }
                    } else {
                        harmoniq_engine::sound_server::UltraLowLatencyServer::start(
                            Arc::clone(&engine),
                            config.clone(),
                            base_options.clone(),
                        )?
                    }
                };

                #[cfg(not(feature = "openasio"))]
                let server = harmoniq_engine::sound_server::UltraLowLatencyServer::start(
                    Arc::clone(&engine),
                    config.clone(),
                    base_options,
                )?;
                let device_name = device.unwrap_or_else(|| server.device_name().to_string());
                Ok(StreamCreation {
                    stream: StreamBackend::Ultra(server),
                    backend: AudioBackend::Harmoniq,
                    device_name,
                    device_id: None,
                    host_label: Some("Harmoniq Ultra".to_string()),
                    sample_rate: engine_rate,
                })
            }
            AudioBackend::PulseAudio | AudioBackend::PipeWire | AudioBackend::Alsa => {
                if !alsa_devices_available() {
                    anyhow::bail!("no ALSA-compatible audio devices detected");
                }
                let host_id = cpal::HostId::Alsa;
                if !cpal::available_hosts().contains(&host_id) {
                    anyhow::bail!("backend {backend} is not available on this system");
                }
                let host = cpal::host_from_id(host_id)?;
                let selector =
                    sanitize_device_for_backend(backend, options.output_device.as_deref());
                Self::build_cpal_stream(
                    Arc::clone(&engine),
                    config,
                    backend,
                    host,
                    host_id,
                    selector.as_deref(),
                )
            }
            AudioBackend::Auto => {
                let host = cpal::default_host();
                let host_id = host.id();
                let resolved = AudioBackend::from_host_id(host_id).unwrap_or(AudioBackend::Auto);
                Self::build_cpal_stream(
                    Arc::clone(&engine),
                    config,
                    resolved,
                    host,
                    host_id,
                    options.output_device.as_deref(),
                )
            }
            other => {
                let host_id = other
                    .host_id()
                    .ok_or_else(|| anyhow!("backend {other} is not supported on this platform"))?;
                let host = cpal::host_from_id(host_id)?;
                Self::build_cpal_stream(
                    Arc::clone(&engine),
                    config,
                    other,
                    host,
                    host_id,
                    options.output_device.as_deref(),
                )
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn start_with_backend(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        options: &AudioRuntimeOptions,
        backend: AudioBackend,
    ) -> anyhow::Result<StreamCreation> {
        let (host, resolved_backend, host_id) = match backend {
            AudioBackend::Auto => {
                let host = cpal::default_host();
                let host_id = host.id();
                let resolved = AudioBackend::from_host_id(host_id).unwrap_or(AudioBackend::Auto);
                (host, resolved, host_id)
            }
            other => {
                let host_id = other
                    .host_id()
                    .ok_or_else(|| anyhow!("backend {other} is not supported on this platform"))?;
                let host = cpal::host_from_id(host_id)?;
                (host, other, host_id)
            }
        };

        Self::build_cpal_stream(
            Arc::clone(&engine),
            config,
            resolved_backend,
            host,
            host_id,
            options.output_device.as_deref(),
        )
    }

    fn build_cpal_stream(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        backend: AudioBackend,
        host: cpal::Host,
        host_id: cpal::HostId,
        selector: Option<&str>,
    ) -> anyhow::Result<StreamCreation> {
        let device = Self::select_output_device(&host, selector)?;
        let (device_name, device_id) = match device.name() {
            Ok(name) => {
                let id = device_identifier_for_backend(backend, host_id, &name);
                (name, Some(id))
            }
            Err(_) => ("unknown device".to_string(), None),
        };

        let supported_config = match Self::select_output_config(&device, config.sample_rate)? {
            Some(config) => config,
            None => device
                .default_output_config()
                .context("failed to query default output config")?,
        };

        let mut stream_config: StreamConfig = supported_config.config();
        stream_config.buffer_size = BufferSize::Fixed(config.block_size as u32);

        if stream_config.channels as usize != config.layout.channels() as usize {
            warn!(
                device_channels = stream_config.channels,
                engine_channels = config.layout.channels(),
                "Device channel count differs from engine layout; excess channels will be silent"
            );
        }

        let err_fn = |err| {
            tracing::error!(?err, "audio stream error");
        };

        let stream = match supported_config.sample_format() {
            SampleFormat::F32 => Self::build_stream::<f32>(
                &device,
                &stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            SampleFormat::I16 => Self::build_stream::<i16>(
                &device,
                &stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            SampleFormat::U16 => Self::build_stream::<u16>(
                &device,
                &stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            SampleFormat::U8 => Self::build_stream::<u8>(
                &device,
                &stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            other => anyhow::bail!("unsupported output sample format: {other:?}"),
        };

        stream
            .play()
            .context("failed to start audio output stream")?;

        Ok(StreamCreation {
            #[cfg(target_os = "linux")]
            stream: StreamBackend::Cpal(stream),
            #[cfg(not(target_os = "linux"))]
            stream,
            backend,
            device_name,
            device_id,
            host_label: Self::host_label_for_backend(backend, Some(host_id)),
            sample_rate: stream_config.sample_rate.0,
        })
    }

    fn select_output_device(
        host: &cpal::Host,
        selector: Option<&str>,
    ) -> anyhow::Result<cpal::Device> {
        if let Some(selector) = selector {
            #[cfg(target_os = "linux")]
            let target = parse_linux_device_selector(selector)
                .map(|(_, name)| name)
                .unwrap_or(selector);
            #[cfg(not(target_os = "linux"))]
            let target = selector;

            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    if device.name().map(|name| name == target).unwrap_or(false) {
                        return Ok(device);
                    }
                }
            }
        }

        host.default_output_device()
            .or_else(|| {
                host.output_devices()
                    .ok()
                    .and_then(|mut devices| devices.next())
            })
            .ok_or_else(|| anyhow!("no audio output device available"))
    }

    fn host_label_for_backend(
        backend: AudioBackend,
        host_id: Option<cpal::HostId>,
    ) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            Some(linux_backend_label(backend, host_id))
        }
        #[cfg(not(target_os = "linux"))]
        {
            host_id.map(|id| id.name().to_string())
        }
    }

    pub fn backend(&self) -> AudioBackend {
        self.backend
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    pub fn host_label(&self) -> Option<&str> {
        self.host_label.as_deref()
    }

    fn select_output_config(
        device: &cpal::Device,
        sample_rate: f32,
    ) -> anyhow::Result<Option<cpal::SupportedStreamConfig>> {
        let desired_rate = cpal::SampleRate(sample_rate.round() as u32);
        let mut configs = device.supported_output_configs()?;
        for config in configs.by_ref() {
            if config.min_sample_rate() <= desired_rate && config.max_sample_rate() >= desired_rate
            {
                return Ok(Some(config.with_sample_rate(desired_rate)));
            }
        }
        Ok(None)
    }

    fn build_stream<T>(
        device: &cpal::Device,
        stream_config: &StreamConfig,
        engine: Arc<Mutex<HarmoniqEngine>>,
        buffer_config: BufferConfig,
        err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
    ) -> anyhow::Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = stream_config.channels as usize;
        let mut facade = RealtimeEngineFacade::new(engine, buffer_config, channels)?;
        let mut callback_handle = facade.callback_handle();
        let stream = device.build_output_stream(
            stream_config,
            move |output: &mut [T], _| {
                facade.render_output(&mut callback_handle, output);
            },
            err_fn,
            None,
        )?;
        Ok(stream)
    }
}

#[derive(Clone)]
pub struct SoundTestSample {
    samples: Vec<Vec<f32>>,
    sample_rate: f32,
}

impl SoundTestSample {
    pub fn load() -> anyhow::Result<Self> {
        Self::from_bytes(include_bytes!(
            "../../../resources/audio/testing_harmoniq_stereo.wav"
        ))
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let cursor = Cursor::new(bytes);
        let mut reader =
            hound::WavReader::new(cursor).context("failed to open sound test audio")?;
        let spec = reader.spec();
        let channels = spec.channels.max(1) as usize;
        let sample_rate = spec.sample_rate as f32;
        let mut samples = vec![Vec::new(); channels];

        for (index, sample) in reader.samples::<i16>().enumerate() {
            let value = sample.context("failed to decode sound test audio")? as f32 / 32768.0;
            let channel = index % channels;
            samples[channel].push(value);
        }

        Ok(Self {
            samples,
            sample_rate,
        })
    }

    pub fn prepare_clip(&self, target_sample_rate: f32) -> AudioClip {
        if self.samples.is_empty() {
            return AudioClip::from_channels(Vec::<Vec<f32>>::new());
        }

        if self.sample_rate <= 0.0 || target_sample_rate <= 0.0 {
            return AudioClip::from_channels(self.samples.clone());
        }

        let ratio = target_sample_rate / self.sample_rate;
        if (ratio - 1.0).abs() < f32::EPSILON {
            return AudioClip::from_channels(self.samples.clone());
        }

        let source_frames = self.samples[0].len();
        if source_frames == 0 {
            return AudioClip::from_channels(self.samples.clone());
        }

        let target_frames = ((source_frames as f32) * ratio).ceil().max(1.0) as usize;
        let mut channels = Vec::with_capacity(self.samples.len());

        for source in &self.samples {
            if source.is_empty() {
                channels.push(vec![0.0; target_frames]);
                continue;
            }

            let mut resampled = Vec::with_capacity(target_frames);
            let max_index = source.len() - 1;
            for frame in 0..target_frames {
                let position = (frame as f32) / ratio;
                let lower = position.floor() as usize;
                let frac = position - lower as f32;
                let lower = lower.min(max_index);
                let upper = (lower + 1).min(max_index);
                let start = source[lower];
                let end = source[upper];
                let value = start + (end - start) * frac;
                resampled.push(value.clamp(-1.0, 1.0));
            }
            channels.push(resampled);
        }

        AudioClip::from_channels(channels)
    }
}

fn device_identifier_for_backend(
    backend: AudioBackend,
    host_id: cpal::HostId,
    name: &str,
) -> String {
    #[cfg(target_os = "linux")]
    {
        match backend {
            AudioBackend::Harmoniq => format!("harmoniq::{name}"),
            #[cfg(feature = "openasio")]
            AudioBackend::OpenAsio => format!("openasio::{name}"),
            AudioBackend::PulseAudio | AudioBackend::PipeWire | AudioBackend::Alsa => {
                linux_device_identifier(cpal::HostId::Alsa, name)
            }
            AudioBackend::Asio => linux_device_identifier(host_id, name),
            AudioBackend::Auto => linux_device_identifier(host_id, name),
            _ => linux_device_identifier(host_id, name),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        name.to_string()
    }
}

pub fn available_backends() -> Vec<(AudioBackend, String)> {
    let mut hosts: Vec<(AudioBackend, String)> = cpal::available_hosts()
        .into_iter()
        .filter_map(|host| {
            AudioBackend::from_host_id(host).map(|backend| (backend, host.name().to_string()))
        })
        .collect();

    if hosts
        .iter()
        .all(|(backend, _)| *backend != AudioBackend::Auto)
    {
        hosts.insert(
            0,
            (AudioBackend::Auto, "Automatic (detect best)".to_string()),
        );
    }

    #[cfg(target_os = "linux")]
    {
        if alsa_devices_available() {
            if hosts
                .iter()
                .all(|(backend, _)| *backend != AudioBackend::Harmoniq)
            {
                hosts.insert(
                    1,
                    (
                        AudioBackend::Harmoniq,
                        "Harmoniq Ultra (custom ALSA server)".to_string(),
                    ),
                );
            }
            if hosts
                .iter()
                .all(|(backend, _)| *backend != AudioBackend::PulseAudio)
            {
                hosts.push((
                    AudioBackend::PulseAudio,
                    "PulseAudio (via ALSA compatibility)".to_string(),
                ));
            }
            if hosts
                .iter()
                .all(|(backend, _)| *backend != AudioBackend::PipeWire)
            {
                hosts.push((
                    AudioBackend::PipeWire,
                    "PipeWire (via ALSA compatibility)".to_string(),
                ));
            }
            if hosts
                .iter()
                .all(|(backend, _)| *backend != AudioBackend::Asio)
            {
                hosts.push((
                    AudioBackend::Asio,
                    "Harmoniq ASIO (ultra low latency)".to_string(),
                ));
            }
            #[cfg(feature = "openasio")]
            if hosts
                .iter()
                .all(|(backend, _)| *backend != AudioBackend::OpenAsio)
            {
                hosts.push((
                    AudioBackend::OpenAsio,
                    "OpenASIO (experimental low latency)".to_string(),
                ));
            }
        }
    }

    hosts
}

#[derive(Debug, Clone)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub label: String,
}

pub fn available_output_devices(backend: AudioBackend) -> anyhow::Result<Vec<OutputDeviceInfo>> {
    #[cfg(target_os = "linux")]
    {
        linux_available_output_devices(backend)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let host = match backend {
            AudioBackend::Auto => cpal::default_host(),
            other => {
                let host_id = other
                    .host_id()
                    .ok_or_else(|| anyhow!("backend {other} is not supported on this platform"))?;
                cpal::host_from_id(host_id)?
            }
        };
        Ok(collect_device_info(&host))
    }
}

#[cfg(not(target_os = "linux"))]
fn collect_device_info(host: &cpal::Host) -> Vec<OutputDeviceInfo> {
    let mut devices = Vec::new();
    if let Ok(mut outputs) = host.output_devices() {
        for device in outputs {
            if let Ok(name) = device.name() {
                devices.push(OutputDeviceInfo {
                    id: name.clone(),
                    label: name,
                });
            }
        }
    }
    if devices.is_empty() {
        if let Some(default) = host.default_output_device() {
            if let Ok(name) = default.name() {
                devices.push(OutputDeviceInfo {
                    id: name.clone(),
                    label: name,
                });
            }
        }
    }
    devices
}

#[cfg(target_os = "linux")]
fn linux_available_output_devices(backend: AudioBackend) -> anyhow::Result<Vec<OutputDeviceInfo>> {
    let mut devices: Vec<OutputDeviceInfo> = Vec::new();
    match backend {
        AudioBackend::Asio => {
            if !alsa_devices_available() {
                return Ok(devices);
            }
            if let Ok(host) = cpal::host_from_id(cpal::HostId::Alsa) {
                for device in enumerate_linux_devices(&host, AudioBackend::Asio, cpal::HostId::Alsa)
                {
                    if !devices.iter().any(|existing| existing.id == device.id) {
                        devices.push(device);
                    }
                }
            }
        }
        AudioBackend::PipeWire | AudioBackend::PulseAudio | AudioBackend::Alsa => {
            if !alsa_devices_available() {
                return Ok(devices);
            }
            let host = cpal::host_from_id(cpal::HostId::Alsa)?;
            devices.extend(enumerate_linux_devices(&host, backend, cpal::HostId::Alsa));
        }
        AudioBackend::Harmoniq => {
            if !alsa_devices_available() {
                devices.push(OutputDeviceInfo {
                    id: "harmoniq::default".to_string(),
                    label: "ALSA default".to_string(),
                });
                return Ok(devices);
            }
            if let Ok(host) = cpal::host_from_id(cpal::HostId::Alsa) {
                devices.extend(enumerate_linux_devices(
                    &host,
                    AudioBackend::Harmoniq,
                    cpal::HostId::Alsa,
                ));
            }
            if devices.is_empty() {
                devices.push(OutputDeviceInfo {
                    id: "harmoniq::default".to_string(),
                    label: "ALSA default".to_string(),
                });
            }
        }
        #[cfg(feature = "openasio")]
        AudioBackend::OpenAsio => {
            // Enumeration requires loading the requested OpenASIO driver; rely on
            // explicit CLI configuration for now.
        }
        AudioBackend::Auto => {
            let host = cpal::default_host();
            devices.extend(enumerate_linux_devices(
                &host,
                AudioBackend::Auto,
                host.id(),
            ));
        }
        other => {
            let host_id = other
                .host_id()
                .ok_or_else(|| anyhow!("backend {other} is not supported on this platform"))?;
            let host = cpal::host_from_id(host_id)?;
            devices.extend(enumerate_linux_devices(&host, other, host_id));
        }
    }
    Ok(devices)
}

#[cfg(target_os = "linux")]
fn enumerate_linux_devices(
    host: &cpal::Host,
    backend: AudioBackend,
    host_id: cpal::HostId,
) -> Vec<OutputDeviceInfo> {
    if !alsa_devices_available() {
        return Vec::new();
    }
    let mut devices = Vec::new();
    if let Ok(outputs) = host.output_devices() {
        for device in outputs {
            if let Ok(name) = device.name() {
                let id = device_identifier_for_backend(backend, host_id, &name);
                let id_ref = id.as_str();
                if !devices
                    .iter()
                    .any(|info: &OutputDeviceInfo| info.id.as_str() == id_ref)
                {
                    devices.push(OutputDeviceInfo { id, label: name });
                }
            }
        }
    }
    if devices.is_empty() {
        if let Some(default) = host.default_output_device() {
            if let Ok(name) = default.name() {
                let id = device_identifier_for_backend(backend, host_id, &name);
                let id_ref = id.as_str();
                if !devices
                    .iter()
                    .any(|info: &OutputDeviceInfo| info.id.as_str() == id_ref)
                {
                    devices.push(OutputDeviceInfo { id, label: name });
                }
            }
        }
    }
    devices
}

pub fn describe_layout(layout: ChannelLayout) -> &'static str {
    match layout {
        ChannelLayout::Mono => "mono",
        ChannelLayout::Stereo => "stereo",
        ChannelLayout::Surround51 => "5.1",
        ChannelLayout::Custom(_) => "custom",
    }
}

#[cfg(target_os = "linux")]
mod linux_asio {
    use std::convert::TryFrom;
    use std::sync::Arc;

    use anyhow::{anyhow, Context};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{BufferSize, SampleFormat, StreamConfig};
    use harmoniq_engine::{AudioBuffer, BufferConfig, HarmoniqEngine};
    use parking_lot::Mutex;
    use tracing::warn;

    pub struct LinuxAsioDriver {
        stream: cpal::Stream,
        device_name: String,
        device_id: String,
        host_id: cpal::HostId,
    }

    impl LinuxAsioDriver {
        pub fn start(
            engine: Arc<Mutex<HarmoniqEngine>>,
            config: BufferConfig,
            preferred_host: Option<cpal::HostId>,
            preferred_device: Option<&str>,
        ) -> anyhow::Result<Self> {
            if !super::alsa_devices_available() {
                anyhow::bail!("no ALSA-compatible audio devices detected");
            }
            let (host, host_id) = select_host(preferred_host)?;
            let device = select_device(&host, preferred_device)
                .ok_or_else(|| anyhow!("no audio output device available for Linux ASIO"))?;

            let device_name = device
                .name()
                .unwrap_or_else(|_| "unknown device".to_string());
            let device_id = super::device_identifier_for_backend(
                super::AudioBackend::Asio,
                host_id,
                &device_name,
            );

            let supported_config = match pick_stream_config(&device, config.sample_rate)? {
                Some(config) => config,
                None => device
                    .default_output_config()
                    .context("failed to query default output config for Linux ASIO")?,
            };

            let mut stream_config: StreamConfig = supported_config.config();
            let block_size = u32::try_from(config.block_size)
                .context("engine block size exceeds CPAL limits")?;
            stream_config.buffer_size = BufferSize::Fixed(block_size);
            let channels = stream_config.channels as usize;
            let engine_channels = config.layout.channels() as usize;

            if channels < engine_channels {
                warn!(
                    device_channels = channels,
                    engine_channels,
                    "Linux ASIO device exposes fewer channels than engine layout; higher channels will be muted"
                );
            } else if channels > engine_channels {
                warn!(
                    device_channels = channels,
                    engine_channels,
                    "Linux ASIO device exposes more channels than engine layout; extra channels will output silence"
                );
            }

            let device_rate = stream_config.sample_rate.0 as f32;
            if (device_rate - config.sample_rate).abs() > 0.5 {
                warn!(
                    engine_rate = config.sample_rate,
                    device_rate, "Linux ASIO device sample rate differs from engine configuration"
                );
            }

            let err_fn = |err| {
                tracing::error!(?err, "Linux ASIO stream error");
            };
            let channels = channels;
            let stream = match supported_config.sample_format() {
                SampleFormat::F32 => {
                    let mut state = CallbackState::new(Arc::clone(&engine), config.clone());
                    device.build_output_stream(
                        &stream_config,
                        move |output: &mut [f32], _| {
                            state.render_into(output, channels);
                        },
                        err_fn,
                        None,
                    )?
                }
                SampleFormat::I16 => {
                    let mut state = CallbackState::new(Arc::clone(&engine), config.clone());
                    device.build_output_stream(
                        &stream_config,
                        move |output: &mut [i16], _| {
                            state.render_into(output, channels);
                        },
                        err_fn,
                        None,
                    )?
                }
                SampleFormat::U16 => {
                    let mut state = CallbackState::new(Arc::clone(&engine), config.clone());
                    device.build_output_stream(
                        &stream_config,
                        move |output: &mut [u16], _| {
                            state.render_into(output, channels);
                        },
                        err_fn,
                        None,
                    )?
                }
                SampleFormat::U8 => {
                    let mut state = CallbackState::new(Arc::clone(&engine), config.clone());
                    device.build_output_stream(
                        &stream_config,
                        move |output: &mut [u8], _| {
                            state.render_into(output, channels);
                        },
                        err_fn,
                        None,
                    )?
                }
                other => anyhow::bail!("unsupported Linux ASIO sample format: {other:?}"),
            };

            stream.play()?;

            let backend_label =
                super::linux_backend_label(super::AudioBackend::Asio, Some(host_id));
            tracing::info!(
                backend = %backend_label,
                device = %device_name,
                sample_format = ?supported_config.sample_format(),
                sample_rate = stream_config.sample_rate.0,
                block_size,
                "Started Linux ASIO audio stream",
            );

            Ok(Self {
                stream,
                device_name,
                device_id,
                host_id,
            })
        }

        pub fn device_name(&self) -> &str {
            &self.device_name
        }

        pub fn device_id(&self) -> &str {
            &self.device_id
        }

        pub fn host_id(&self) -> cpal::HostId {
            self.host_id
        }
    }

    fn select_host(preferred: Option<cpal::HostId>) -> anyhow::Result<(cpal::Host, cpal::HostId)> {
        let available = cpal::available_hosts();
        if let Some(preferred) = preferred {
            if available.contains(&preferred) {
                if let Ok(host) = cpal::host_from_id(preferred) {
                    return Ok((host, preferred));
                }
            }
        }
        if available.contains(&cpal::HostId::Alsa) {
            if let Ok(host) = cpal::host_from_id(cpal::HostId::Alsa) {
                return Ok((host, cpal::HostId::Alsa));
            }
        }
        let host = cpal::default_host();
        let host_id = host.id();
        Ok((host, host_id))
    }

    fn select_device(host: &cpal::Host, preferred: Option<&str>) -> Option<cpal::Device> {
        if let Some(selector) = preferred {
            let target = super::parse_linux_device_selector(selector)
                .map(|(_, name)| name)
                .unwrap_or(selector);
            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    if device.name().map(|name| name == target).unwrap_or(false) {
                        return Some(device);
                    }
                }
            }
        }
        host.default_output_device().or_else(|| {
            host.output_devices()
                .ok()
                .and_then(|mut devices| devices.next())
        })
    }

    fn pick_stream_config(
        device: &cpal::Device,
        sample_rate: f32,
    ) -> anyhow::Result<Option<cpal::SupportedStreamConfig>> {
        let desired = cpal::SampleRate(sample_rate.round() as u32);
        let mut configs = device.supported_output_configs()?;
        for config in configs.by_ref() {
            if config.min_sample_rate() <= desired && config.max_sample_rate() >= desired {
                return Ok(Some(config.with_sample_rate(desired)));
            }
        }
        Ok(None)
    }

    struct CallbackState {
        engine: Arc<Mutex<HarmoniqEngine>>,
        buffer: AudioBuffer,
        cursor: usize,
        error_reported: bool,
    }

    impl CallbackState {
        fn new(engine: Arc<Mutex<HarmoniqEngine>>, config: BufferConfig) -> Self {
            let buffer = AudioBuffer::from_config(&config);
            Self {
                engine,
                cursor: buffer.len(),
                buffer,
                error_reported: false,
            }
        }

        fn render_into<T>(&mut self, output: &mut [T], channels: usize)
        where
            T: cpal::SizedSample + cpal::FromSample<f32>,
        {
            if channels == 0 {
                return;
            }

            let total_frames = output.len() / channels;
            let mut frame_index = 0usize;

            while frame_index < total_frames {
                if self.cursor >= self.buffer.len() {
                    if let Err(err) = self.refill() {
                        if !self.error_reported {
                            tracing::warn!(
                                ?err,
                                "Linux ASIO engine processing failed; outputting silence"
                            );
                            self.error_reported = true;
                        }
                        Self::fill_silence(output, frame_index * channels);
                        return;
                    }
                    self.error_reported = false;
                }

                let available_frames = self.buffer.len().saturating_sub(self.cursor);
                if available_frames == 0 {
                    Self::fill_silence(output, frame_index * channels);
                    return;
                }

                let frames_to_copy = (total_frames - frame_index).min(available_frames);
                let engine_channels = self.buffer.channel_count();

                for local_frame in 0..frames_to_copy {
                    let src_index = self.cursor + local_frame;
                    let dst_index = frame_index + local_frame;
                    for channel in 0..channels {
                        let value = if channel < engine_channels {
                            self.buffer
                                .channel(channel)
                                .get(src_index)
                                .copied()
                                .unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        output[dst_index * channels + channel] = T::from_sample(value);
                    }
                }

                self.cursor += frames_to_copy;
                frame_index += frames_to_copy;
            }
        }

        fn refill(&mut self) -> anyhow::Result<()> {
            self.buffer.clear();
            {
                let mut engine = self.engine.lock();
                engine.process_block(&mut self.buffer)?;
            }
            self.cursor = 0;
            Ok(())
        }

        fn fill_silence<T>(output: &mut [T], start_sample: usize)
        where
            T: cpal::SizedSample + cpal::FromSample<f32>,
        {
            for sample in &mut output[start_sample..] {
                *sample = T::from_sample(0.0);
            }
        }
    }
}

pub struct StockSoundLibrary {
    sounds: HashMap<String, SoundTestSample>,
}

impl StockSoundLibrary {
    pub fn load() -> anyhow::Result<Self> {
        let mut sounds = HashMap::new();
        for (name, bytes) in STOCK_SOUNDS {
            let sample = SoundTestSample::from_bytes(bytes)
                .with_context(|| format!("failed to load stock sound: {name}"))?;
            sounds.insert(name.to_ascii_lowercase(), sample);
        }

        Ok(Self { sounds })
    }

    pub fn prepare_clip(&self, name: &str, target_sample_rate: f32) -> Option<AudioClip> {
        let key = name.to_ascii_lowercase();
        let sample = self.sounds.get(&key)?;
        Some(sample.prepare_clip(target_sample_rate))
    }
}

const STOCK_SOUNDS: &[(&str, &[u8])] = &[
    (
        "Sunrise Reverie",
        include_bytes!("../../../resources/audio/stock/sunrise_reverie.wav"),
    ),
    (
        "Midnight Drive",
        include_bytes!("../../../resources/audio/stock/midnight_drive.wav"),
    ),
    (
        "Neon Skies",
        include_bytes!("../../../resources/audio/stock/neon_skies.wav"),
    ),
    (
        "Lo-Fi Sketchbook",
        include_bytes!("../../../resources/audio/stock/lofi_sketchbook.wav"),
    ),
    (
        "Festival Sparks",
        include_bytes!("../../../resources/audio/stock/festival_sparks.wav"),
    ),
];

#[cfg(all(feature = "openasio", target_os = "linux"))]
mod openasio_rt {
    use super::{alsa_devices_available, AudioRuntimeOptions};
    use anyhow::{bail, Context, Result};
    use crossbeam::queue::ArrayQueue;
    use harmoniq_engine::backend::openasio::OpenAsioBackend;
    use harmoniq_engine::backend::{EngineRt, StreamConfig as EngineStreamConfig};
    use harmoniq_engine::buffers::{AudioView, AudioViewMut};
    use harmoniq_engine::{AudioBuffer, BufferConfig, HarmoniqEngine};
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread::{self, JoinHandle};
    use tracing::{error, warn};

    pub struct OpenAsioHandle {
        backend: Option<OpenAsioBackend>,
        device_name: String,
        device_id: Option<String>,
    }

    impl OpenAsioHandle {
        pub fn start(
            engine: Arc<Mutex<HarmoniqEngine>>,
            config: BufferConfig,
            options: &AudioRuntimeOptions,
        ) -> Result<Self> {
            if !alsa_devices_available() {
                bail!("no ALSA-compatible audio devices detected");
            }

            let default_driver = if cfg!(debug_assertions) {
                "target/debug/libopenasio_driver_cpal.so"
            } else {
                "target/release/libopenasio_driver_cpal.so"
            };
            let driver_path = options
                .openasio_driver
                .clone()
                .unwrap_or_else(|| default_driver.to_string());

            let device_selection = options
                .openasio_device
                .clone()
                .filter(|name| !name.is_empty());
            let device_name = device_selection
                .clone()
                .unwrap_or_else(|| "default".to_string());

            let out_channels = u16::from(config.layout.channels().max(1));
            let sample_rate = options
                .openasio_sample_rate
                .unwrap_or_else(|| config.sample_rate.round() as u32)
                .max(1);
            let buffer_frames = options
                .openasio_buffer_frames
                .unwrap_or(config.block_size as u32)
                .max(1);
            let desired =
                EngineStreamConfig::new(sample_rate, buffer_frames, 0, out_channels, true);

            let rt = RtProcess::new(
                Arc::clone(&engine),
                config.clone(),
                out_channels as usize,
                buffer_frames as usize,
            )?;

            let mut backend = OpenAsioBackend::new(driver_path, device_selection.clone(), desired);
            backend.start(Box::new(rt))?;

            let device_id = Some(format!("openasio::{}", device_name));

            Ok(Self {
                backend: Some(backend),
                device_name,
                device_id,
            })
        }

        pub fn device_name(&self) -> &str {
            &self.device_name
        }

        pub fn device_id(&self) -> Option<&str> {
            self.device_id.as_deref()
        }
    }

    impl Drop for OpenAsioHandle {
        fn drop(&mut self) {
            if let Some(mut backend) = self.backend.take() {
                backend.stop();
            }
        }
    }

    struct RtProcess {
        queue: Arc<ArrayQueue<f32>>,
        running: Arc<AtomicBool>,
        out_channels: usize,
        render_thread: Option<JoinHandle<()>>,
    }

    impl RtProcess {
        fn new(
            engine: Arc<Mutex<HarmoniqEngine>>,
            config: BufferConfig,
            out_channels: usize,
            queue_frames: usize,
        ) -> Result<Self> {
            let channels = out_channels.max(1);
            let frames = queue_frames.max(1);
            let capacity = frames
                .saturating_mul(channels)
                .saturating_mul(4)
                .max(channels);
            let queue = Arc::new(ArrayQueue::new(capacity));
            for _ in 0..capacity {
                queue.force_push(0.0);
            }
            let running = Arc::new(AtomicBool::new(true));
            let render_queue = Arc::clone(&queue);
            let render_running = Arc::clone(&running);
            let render_engine = Arc::clone(&engine);
            let render_config = config.clone();
            let render_out_channels = out_channels;
            let render_thread = thread::Builder::new()
                .name("harmoniq-openasio-render".into())
                .spawn(move || {
                    run_render_loop(
                        render_engine,
                        render_config,
                        render_queue,
                        render_running,
                        render_out_channels,
                    );
                })
                .context("failed to spawn OpenASIO render thread")?;
            Ok(Self {
                queue,
                running,
                out_channels,
                render_thread: Some(render_thread),
            })
        }
    }

    impl EngineRt for RtProcess {
        fn process(
            &mut self,
            _inputs: AudioView<'_>,
            mut outputs: AudioViewMut<'_>,
            frames: u32,
        ) -> bool {
            let frames = frames as usize;
            let channels = self.out_channels;
            if frames == 0 || channels == 0 {
                return true;
            }
            let total_samples = frames.saturating_mul(channels);
            if let Some(out) = outputs.interleaved_mut() {
                let len = out.len().min(total_samples);
                for sample in &mut out[..len] {
                    *sample = self.queue.pop().unwrap_or(0.0);
                }
                for sample in &mut out[len..] {
                    *sample = 0.0;
                }
            } else if let Some(mut planar) = outputs.planar() {
                let planes = planar.planes();
                let plane_count = planes.len();
                let frames_to_write = frames.min(planar.frames());
                let ptr = planes.as_mut_ptr();
                for frame_idx in 0..frames_to_write {
                    for ch in 0..plane_count {
                        let sample = self.queue.pop().unwrap_or(0.0);
                        if ch < channels {
                            let plane_ptr = unsafe { *ptr.add(ch) };
                            if !plane_ptr.is_null() {
                                unsafe {
                                    *plane_ptr.add(frame_idx) = sample;
                                }
                            }
                        }
                    }
                }
                let consumed = frames_to_write.saturating_mul(plane_count);
                for _ in consumed..total_samples {
                    let _ = self.queue.pop();
                }
            } else {
                for _ in 0..total_samples {
                    let _ = self.queue.pop();
                }
            }
            true
        }
    }

    impl Drop for RtProcess {
        fn drop(&mut self) {
            self.running.store(false, Ordering::Relaxed);
            if let Some(handle) = self.render_thread.take() {
                if let Err(err) = handle.join() {
                    error!(?err, "OpenASIO render thread terminated unexpectedly");
                }
            }
        }
    }

    fn run_render_loop(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        queue: Arc<ArrayQueue<f32>>,
        running: Arc<AtomicBool>,
        out_channels: usize,
    ) {
        let mut buffer = AudioBuffer::from_config(&config);
        let mut interleaved = vec![0.0f32; out_channels.max(1) * config.block_size.max(1)];
        while running.load(Ordering::Relaxed) {
            let result = {
                let mut guard = engine.lock();
                guard.process_block(&mut buffer)
            };
            if let Err(err) = result {
                warn!(?err, "engine processing failed in OpenASIO render loop");
                buffer.clear();
            }
            let frames = buffer.len();
            if frames == 0 {
                continue;
            }
            let available_channels = buffer.channel_count();
            let required = frames.saturating_mul(out_channels.max(1));
            if interleaved.len() < required {
                interleaved.resize(required, 0.0);
            }
            for frame_idx in 0..frames {
                for ch in 0..out_channels {
                    let value = if ch < available_channels {
                        buffer.channel(ch).get(frame_idx).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    interleaved[frame_idx * out_channels + ch] = value;
                }
            }
            for &sample in &interleaved[..required] {
                queue.force_push(sample);
            }
        }
    }
}
