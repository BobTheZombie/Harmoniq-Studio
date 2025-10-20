use std::fmt;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};
use harmoniq_engine::{
    AudioBuffer, BufferConfig, ChannelLayout, EngineCommandQueue, HarmoniqEngine,
};
use parking_lot::Mutex;
use tracing::{info, warn};

use crate::midi::{open_midi_input, MidiConnection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum AudioBackend {
    Auto,
    Alsa,
    Jack,
    PulseAudio,
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
            cpal::HostId::Alsa => Some(Self::Alsa),
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
            AudioBackend::Asio => "ASIO",
            AudioBackend::Asio4All => "ASIO4ALL",
            AudioBackend::Wasapi => "WASAPI",
            AudioBackend::CoreAudio => "CoreAudio",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Clone)]
pub struct AudioRuntimeOptions {
    pub backend: AudioBackend,
    pub midi_input: Option<String>,
    pub enable_audio: bool,
}

impl AudioRuntimeOptions {
    pub fn new(backend: AudioBackend, midi_input: Option<String>, enable_audio: bool) -> Self {
        Self {
            backend,
            midi_input,
            enable_audio,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enable_audio
    }
}

pub struct RealtimeAudio {
    _stream: cpal::Stream,
    _midi: Option<MidiConnection>,
    backend: AudioBackend,
    device_name: String,
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

        let (host, backend) = match options.backend {
            AudioBackend::Auto => (cpal::default_host(), AudioBackend::Auto),
            AudioBackend::PulseAudio => {
                #[cfg(target_os = "linux")]
                {
                    (
                        cpal::host_from_id(cpal::HostId::Alsa)?,
                        AudioBackend::PulseAudio,
                    )
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(anyhow!("PulseAudio backend is only available on Linux"));
                }
            }
            requested => {
                let host_id = requested.host_id().ok_or_else(|| {
                    anyhow!("backend {requested} is not supported on this platform")
                })?;
                let host = cpal::host_from_id(host_id)?;
                (host, requested)
            }
        };

        let device = host
            .default_output_device()
            .or_else(|| {
                host.output_devices()
                    .ok()
                    .and_then(|mut devices| devices.next())
            })
            .ok_or_else(|| anyhow!("no audio output device available"))?;
        let device_name = device
            .name()
            .unwrap_or_else(|_| "unknown device".to_string());

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

        stream.play()?;
        info!(backend = ?backend, device = %device_name, "Started realtime audio");

        let midi = open_midi_input(options.midi_input.clone(), command_queue)
            .context("failed to initialise MIDI input")?;

        Ok(Self {
            _stream: stream,
            _midi: midi,
            backend,
            device_name,
        })
    }

    pub fn backend(&self) -> AudioBackend {
        self.backend
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
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
        let samples_per_frame = channels;
        if samples_per_frame == 0 {
            return;
        }
        let mut frame_cursor = 0usize;
        let total_frames = output.len() / samples_per_frame;
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

pub fn available_backends() -> Vec<(AudioBackend, String)> {
    let mut hosts: Vec<(AudioBackend, String)> = cpal::available_hosts()
        .into_iter()
        .filter_map(|host| {
            AudioBackend::from_host_id(host).map(|backend| (backend, host.name().to_string()))
        })
        .collect();

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
    }

    hosts
}

pub fn describe_layout(layout: ChannelLayout) -> &'static str {
    match layout {
        ChannelLayout::Mono => "mono",
        ChannelLayout::Stereo => "stereo",
        ChannelLayout::Surround51 => "5.1",
        ChannelLayout::Custom(_) => "custom",
    }
}
