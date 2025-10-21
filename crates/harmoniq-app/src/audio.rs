use std::fmt;
use std::sync::Arc;

#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use std::path::Path;

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommandQueue, HarmoniqEngine,
};
use parking_lot::Mutex;
use tracing::{info, warn};

#[cfg(target_os = "linux")]
use linux_asio::LinuxAsioDriver;

use crate::midi::{open_midi_input, MidiConnection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum AudioBackend {
    Auto,
    Alsa,
    Jack,
    PulseAudio,
    PipeWire,
    Asio,
    Asio4All,
    Wasapi,
    CoreAudio,
}

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
            AudioBackend::Jack => {
                #[cfg(target_os = "linux")]
                {
                    Some(cpal::HostId::Jack)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    None
                }
            }
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
            #[cfg(target_os = "linux")]
            cpal::HostId::Jack => Some(Self::Jack),
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
            AudioBackend::Jack => "JACK",
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
    match host {
        cpal::HostId::Jack => "jack".to_string(),
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
        "jack" => cpal::HostId::Jack,
        "alsa" | "pipewire" | "pulseaudio" => cpal::HostId::Alsa,
        other if other.eq_ignore_ascii_case("jack") => cpal::HostId::Jack,
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
        AudioBackend::Jack => Some(cpal::HostId::Jack),
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
                if let Some(host) = host {
                    (Some(host), Some(linux_device_identifier(host, selector)))
                } else {
                    (None, None)
                }
            }
        }
        _ => (select_default_linux_asio_host(), None),
    }
}

#[cfg(target_os = "linux")]
fn select_default_linux_asio_host() -> Option<cpal::HostId> {
    let available = cpal::available_hosts();
    if available.contains(&cpal::HostId::Jack) {
        Some(cpal::HostId::Jack)
    } else if available.contains(&cpal::HostId::Alsa) {
        Some(cpal::HostId::Alsa)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_backend_label(backend: AudioBackend, host_id: Option<cpal::HostId>) -> String {
    match backend {
        AudioBackend::PipeWire => "PipeWire (via ALSA)".to_string(),
        AudioBackend::PulseAudio => "PulseAudio (via ALSA)".to_string(),
        AudioBackend::Alsa => "ALSA".to_string(),
        AudioBackend::Jack => "JACK".to_string(),
        AudioBackend::Asio => match host_id {
            Some(cpal::HostId::Jack) => "Linux ASIO (JACK)".to_string(),
            Some(cpal::HostId::Alsa) => "Linux ASIO (ALSA)".to_string(),
            _ => "Linux ASIO".to_string(),
        },
        other => other.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct AudioRuntimeOptions {
    pub backend: AudioBackend,
    pub midi_input: Option<String>,
    pub enable_audio: bool,
    pub output_device: Option<String>,
}

impl AudioRuntimeOptions {
    pub fn new(backend: AudioBackend, midi_input: Option<String>, enable_audio: bool) -> Self {
        Self {
            backend,
            midi_input,
            enable_audio,
            output_device: None,
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
}

impl RealtimeAudio {
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
        } = stream_info;
        #[cfg(not(target_os = "linux"))]
        let StreamCreation {
            stream,
            backend,
            device_name,
            device_id,
            host_label,
        } = stream_info;

        if let Some(ref host) = host_label {
            info!(backend = %backend, host = %host, device = %device_name, "Started realtime audio");
        } else {
            info!(backend = %backend, device = %device_name, "Started realtime audio");
        }

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
                {
                    let mut push_unique = |backend: AudioBackend| {
                        if !candidates.contains(&backend) {
                            candidates.push(backend);
                        }
                    };

                    let selection_host = options
                        .output_device
                        .as_deref()
                        .and_then(|sel| parse_linux_device_selector(sel).map(|(host, _)| host));

                    if let Some(host) = selection_host {
                        match host {
                            cpal::HostId::Jack => push_unique(AudioBackend::Jack),
                            cpal::HostId::Alsa => {
                                if is_pipewire_active() {
                                    push_unique(AudioBackend::PipeWire);
                                }
                                if is_pulseaudio_active() {
                                    push_unique(AudioBackend::PulseAudio);
                                }
                                push_unique(AudioBackend::Alsa);
                            }
                            _ => {}
                        }
                    }

                    let available = cpal::available_hosts();
                    let has_alsa = available.contains(&cpal::HostId::Alsa);
                    let has_jack = available.contains(&cpal::HostId::Jack);

                    if has_alsa && is_pulseaudio_active() {
                        push_unique(AudioBackend::PulseAudio);
                    }
                    if has_alsa && is_pipewire_active() {
                        push_unique(AudioBackend::PipeWire);
                    }
                    if has_jack {
                        push_unique(AudioBackend::Jack);
                    }
                    if has_alsa {
                        push_unique(AudioBackend::Alsa);
                    }
                    if select_default_linux_asio_host().is_some() {
                        push_unique(AudioBackend::Asio);
                    }
                    if has_alsa {
                        push_unique(AudioBackend::PulseAudio);
                        push_unique(AudioBackend::PipeWire);
                    }
                    if candidates.is_empty() {
                        push_unique(AudioBackend::Auto);
                    }
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
        match backend {
            AudioBackend::Asio => {
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
                })
            }
            AudioBackend::PulseAudio
            | AudioBackend::PipeWire
            | AudioBackend::Alsa
            | AudioBackend::Jack => {
                let host_id = match backend {
                    AudioBackend::Jack => cpal::HostId::Jack,
                    _ => cpal::HostId::Alsa,
                };
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
                stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            SampleFormat::I16 => Self::build_stream::<i16>(
                &device,
                stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            SampleFormat::U16 => Self::build_stream::<u16>(
                &device,
                stream_config,
                Arc::clone(&engine),
                config.clone(),
                err_fn,
            )?,
            other => anyhow::bail!("unsupported output sample format: {other:?}"),
        };

        Ok(StreamCreation {
            #[cfg(target_os = "linux")]
            stream: StreamBackend::Cpal(stream),
            #[cfg(not(target_os = "linux"))]
            stream,
            backend,
            device_name,
            device_id,
            host_label: Self::host_label_for_backend(backend, Some(host_id)),
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

            if let Ok(mut devices) = host.output_devices() {
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
        stream_config: StreamConfig,
        engine: Arc<Mutex<HarmoniqEngine>>,
        buffer_config: BufferConfig,
        err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
    ) -> anyhow::Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = stream_config.channels as usize;
        let mut local_buffer = AudioBuffer::from_config(buffer_config);
        let stream = device.build_output_stream(
            &stream_config,
            move |output: &mut [T], _| {
                Self::render_output(&engine, &mut local_buffer, channels, output);
            },
            err_fn,
            None,
        )?;
        Ok(stream)
    }

    fn render_output<T>(
        engine: &Arc<Mutex<HarmoniqEngine>>,
        buffer: &mut AudioBuffer,
        channels: usize,
        output: &mut [T],
    ) where
        T: SizedSample + FromSample<f32>,
    {
        if channels == 0 {
            return;
        }
        let mut frame_cursor = 0usize;
        let total_frames = output.len() / channels;
        let silence = T::from_sample(0.0);

        while frame_cursor < total_frames {
            buffer.clear();
            let result = {
                let mut engine = engine.lock();
                engine.process_block(buffer)
            };
            if let Err(err) = result {
                warn!(?err, "engine processing failed during audio callback");
                for sample in output.iter_mut() {
                    *sample = silence;
                }
                return;
            }

            let available_frames = buffer.len();
            let samples = buffer.as_slice();
            let mut copied = 0usize;
            while copied < available_frames && frame_cursor < total_frames {
                for channel in 0..channels {
                    let value = samples
                        .get(channel)
                        .and_then(|chan| chan.get(copied))
                        .copied()
                        .unwrap_or(0.0);
                    let sample_index = (frame_cursor * channels) + channel;
                    if sample_index < output.len() {
                        output[sample_index] = T::from_sample(value);
                    }
                }
                frame_cursor += 1;
                copied += 1;
            }

            if copied == 0 {
                break;
            }
        }

        let produced_samples = total_frames * channels;
        if produced_samples < output.len() {
            for sample in &mut output[produced_samples..] {
                *sample = silence;
            }
        }
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
            AudioBackend::Jack => linux_device_identifier(cpal::HostId::Jack, name),
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
    let mut devices = Vec::new();
    match backend {
        AudioBackend::Asio => {
            if let Ok(host) = cpal::host_from_id(cpal::HostId::Jack) {
                devices.extend(enumerate_linux_devices(
                    &host,
                    AudioBackend::Asio,
                    cpal::HostId::Jack,
                ));
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
            let host = cpal::host_from_id(cpal::HostId::Alsa)?;
            devices.extend(enumerate_linux_devices(&host, backend, cpal::HostId::Alsa));
        }
        AudioBackend::Jack => {
            let host = cpal::host_from_id(cpal::HostId::Jack)?;
            devices.extend(enumerate_linux_devices(&host, backend, cpal::HostId::Jack));
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
    let mut devices = Vec::new();
    if let Ok(mut outputs) = host.output_devices() {
        for device in outputs {
            if let Ok(name) = device.name() {
                let id = device_identifier_for_backend(backend, host_id, &name);
                if !devices.iter().any(|info| info.id == id) {
                    devices.push(OutputDeviceInfo { id, label: name });
                }
            }
        }
    }
    if devices.is_empty() {
        if let Some(default) = host.default_output_device() {
            if let Ok(name) = default.name() {
                let id = device_identifier_for_backend(backend, host_id, &name);
                if !devices.iter().any(|info| info.id == id) {
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
        for candidate in [cpal::HostId::Jack, cpal::HostId::Alsa] {
            if available.contains(&candidate) {
                if let Ok(host) = cpal::host_from_id(candidate) {
                    return Ok((host, candidate));
                }
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
            if let Ok(mut devices) = host.output_devices() {
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
            let buffer = AudioBuffer::from_config(config);
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
                let source_channels = self.buffer.as_slice();
                let engine_channels = source_channels.len();

                for local_frame in 0..frames_to_copy {
                    let src_index = self.cursor + local_frame;
                    let dst_index = frame_index + local_frame;
                    for channel in 0..channels {
                        let value = if channel < engine_channels {
                            source_channels[channel]
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
