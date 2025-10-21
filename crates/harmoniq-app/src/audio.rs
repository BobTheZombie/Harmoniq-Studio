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

#[cfg(target_os = "linux")]
use linux_asio::LinuxAsioDriver;

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
    #[cfg(target_os = "linux")]
    _stream: StreamBackend,
    #[cfg(not(target_os = "linux"))]
    _stream: cpal::Stream,
    _midi: Option<MidiConnection>,
    backend: AudioBackend,
    device_name: String,
}

#[cfg(target_os = "linux")]
enum StreamBackend {
    Cpal(cpal::Stream),
    LinuxAsio(LinuxAsioDriver),
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

        #[cfg(target_os = "linux")]
        if matches!(options.backend, AudioBackend::Asio) {
            let driver = LinuxAsioDriver::start(Arc::clone(&engine), config.clone())
                .context("failed to start Linux ASIO driver")?;
            let device_name = driver.device_name().to_string();
            let midi = open_midi_input(options.midi_input.clone(), command_queue)
                .context("failed to initialise MIDI input")?;
            info!(
                backend = %AudioBackend::Asio,
                device = %device_name,
                "Started realtime audio via Linux ASIO driver"
            );
            return Ok(Self {
                _stream: StreamBackend::LinuxAsio(driver),
                _midi: midi,
                backend: AudioBackend::Asio,
                device_name,
            });
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
            #[cfg(target_os = "linux")]
            _stream: StreamBackend::Cpal(stream),
            #[cfg(not(target_os = "linux"))]
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
    }

    impl LinuxAsioDriver {
        pub fn start(
            engine: Arc<Mutex<HarmoniqEngine>>,
            config: BufferConfig,
        ) -> anyhow::Result<Self> {
            let (host, host_label) = select_host()?;
            let device = host
                .default_output_device()
                .or_else(|| {
                    host.output_devices()
                        .ok()
                        .and_then(|mut devices| devices.next())
                })
                .ok_or_else(|| anyhow!("no audio output device available for Linux ASIO"))?;

            let device_name = device
                .name()
                .unwrap_or_else(|_| "unknown device".to_string());

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

            tracing::info!(
                backend = %host_label,
                device = %device_name,
                sample_format = ?supported_config.sample_format(),
                sample_rate = stream_config.sample_rate.0,
                block_size,
                "Started Linux ASIO audio stream"
            );

            Ok(Self {
                stream,
                device_name,
            })
        }

        pub fn device_name(&self) -> &str {
            &self.device_name
        }
    }

    fn select_host() -> anyhow::Result<(cpal::Host, String)> {
        let available = cpal::available_hosts();
        for candidate in [cpal::HostId::Jack, cpal::HostId::Alsa] {
            if available.contains(&candidate) {
                if let Ok(host) = cpal::host_from_id(candidate) {
                    return Ok((host, candidate.name().to_string()));
                }
            }
        }
        let host = cpal::default_host();
        let host_name = host.id().name().to_string();
        Ok((host, host_name))
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
