use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Sample, SampleFormat, StreamConfig};
use crossbeam::channel::{self, Receiver, Sender};
use crossbeam::queue::ArrayQueue;
use parking_lot::Mutex;

use crate::{AudioBuffer, BufferConfig, HarmoniqEngine};

/// Options for configuring the Harmoniq ultra low latency sound server.
#[derive(Debug, Clone)]
pub struct UltraLowLatencyOptions {
    /// Optional human readable device name. When omitted the default output
    /// device exposed by the ALSA backend is used.
    pub device: Option<String>,
    /// Optional fixed buffer size override in frames.
    pub buffer_frames: Option<u32>,
    /// Number of buffers retained in the queue between the render thread and
    /// the output callback.
    pub queue_depth: usize,
    /// Optional thread priority for the render worker.
    pub realtime_priority: Option<i32>,
}

impl Default for UltraLowLatencyOptions {
    fn default() -> Self {
        Self {
            device: None,
            buffer_frames: None,
            queue_depth: 4,
            realtime_priority: Some(70),
        }
    }
}

impl UltraLowLatencyOptions {
    pub fn with_device(mut self, device: Option<String>) -> Self {
        self.device = device;
        self
    }

    pub fn with_buffer_frames(mut self, frames: Option<u32>) -> Self {
        self.buffer_frames = frames;
        self
    }

    pub fn with_queue_depth(mut self, depth: usize) -> Self {
        self.queue_depth = depth.max(2);
        self
    }

    pub fn with_realtime_priority(mut self, priority: Option<i32>) -> Self {
        self.realtime_priority = priority;
        self
    }
}

enum ControlMessage {
    Stop,
}

/// Handle for the Harmoniq ultra low latency sound server.
pub struct UltraLowLatencyServer {
    stream: cpal::Stream,
    render_thread: Option<JoinHandle<()>>,
    control: Sender<ControlMessage>,
    running: Arc<AtomicBool>,
    device_name: String,
    config: BufferConfig,
}

impl UltraLowLatencyServer {
    pub fn start(
        engine: Arc<Mutex<HarmoniqEngine>>,
        config: BufferConfig,
        options: UltraLowLatencyOptions,
    ) -> anyhow::Result<Self> {
        if !alsa_devices_available() {
            anyhow::bail!("no ALSA-compatible audio devices detected");
        }

        let host = cpal::host_from_id(cpal::HostId::Alsa).unwrap_or_else(|_| cpal::default_host());
        let device = select_device(&host, options.device.as_deref())
            .context("failed to select ALSA output device")?;
        let device_name = device
            .name()
            .unwrap_or_else(|_| "unknown device".to_string());

        let supported = select_config(&device, config.sample_rate)
            .context("failed to negotiate device configuration")?;
        let mut stream_config: StreamConfig = supported.config();
        stream_config.buffer_size =
            BufferSize::Fixed(options.buffer_frames.unwrap_or(config.block_size as u32));

        let channels = stream_config.channels as usize;
        if channels != config.layout.channels() as usize {
            tracing::warn!(
                device_channels = channels,
                engine_channels = config.layout.channels(),
                "Device channel count differs from engine layout; excess channels will be silent"
            );
        }

        let queue_capacity = (config.block_size * channels).max(1) * options.queue_depth;
        let queue = Arc::new(ArrayQueue::new(queue_capacity));
        let running = Arc::new(AtomicBool::new(true));

        let render_queue = Arc::clone(&queue);
        let render_running = Arc::clone(&running);
        let (control_tx, control_rx) = channel::bounded(1);
        let render_config = config.clone();
        let render_options = options.clone();

        let stream = build_stream(
            &device,
            &stream_config,
            supported.sample_format(),
            Arc::clone(&queue),
            Arc::clone(&running),
        )?;
        stream.play()?;

        let render_handle = thread::Builder::new()
            .name("harmoniq-ultra-render".into())
            .spawn(move || {
                if let Err(err) = render_loop(
                    engine,
                    render_config,
                    render_queue,
                    control_rx,
                    render_running,
                    render_options,
                ) {
                    tracing::error!(?err, "sound server render loop terminated with error");
                }
            })
            .context("failed to spawn render thread")?;

        Ok(Self {
            stream,
            render_thread: Some(render_handle),
            control: control_tx,
            running,
            device_name,
            config,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn config(&self) -> &BufferConfig {
        &self.config
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn shutdown(mut self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.control.send(ControlMessage::Stop);
        self.stream.pause()?;
        if let Some(handle) = self.render_thread.take() {
            handle
                .join()
                .map_err(|_| anyhow!("render thread panicked"))?;
        }
        Ok(())
    }
}

impl Drop for UltraLowLatencyServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.control.send(ControlMessage::Stop);
        let _ = self.stream.pause();
        if let Some(handle) = self.render_thread.take() {
            let _ = handle.join();
        }
    }
}

fn select_device(host: &cpal::Host, selection: Option<&str>) -> anyhow::Result<cpal::Device> {
    if let Some(selector) = selection {
        let target = selector
            .split_once("::")
            .map(|(_, name)| name)
            .unwrap_or(selector);
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if device.name().map(|name| name == target).unwrap_or(false) {
                    return Ok(device);
                }
            }
        }
    }

    host.default_output_device()
        .or_else(|| host.output_devices().ok().and_then(|mut list| list.next()))
        .ok_or_else(|| anyhow!("no output device available"))
}

fn select_config(
    device: &cpal::Device,
    sample_rate: f32,
) -> anyhow::Result<cpal::SupportedStreamConfig> {
    let desired_rate = cpal::SampleRate(sample_rate.round() as u32);
    let mut configs = device.supported_output_configs()?;
    for config in configs.by_ref() {
        if config.min_sample_rate() <= desired_rate && config.max_sample_rate() >= desired_rate {
            return Ok(config.with_sample_rate(desired_rate));
        }
    }
    device
        .default_output_config()
        .context("output device does not expose a compatible configuration")
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    queue: Arc<ArrayQueue<f32>>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let err_fn = |err| tracing::error!(?err, "audio stream error");

    match sample_format {
        SampleFormat::F32 => Ok(device.build_output_stream(
            config,
            move |output: &mut [f32], _| fill_from_queue(output, &queue, &running),
            err_fn,
            None,
        )?),
        SampleFormat::I16 => Ok(device.build_output_stream(
            config,
            move |output: &mut [i16], _| fill_from_queue(output, &queue, &running),
            err_fn,
            None,
        )?),
        SampleFormat::U16 => Ok(device.build_output_stream(
            config,
            move |output: &mut [u16], _| fill_from_queue(output, &queue, &running),
            err_fn,
            None,
        )?),
        SampleFormat::U8 => Ok(device.build_output_stream(
            config,
            move |output: &mut [u8], _| fill_from_queue(output, &queue, &running),
            err_fn,
            None,
        )?),
        other => Err(anyhow!("unsupported sample format: {other:?}")),
    }
}

fn fill_from_queue<T>(output: &mut [T], queue: &ArrayQueue<f32>, running: &AtomicBool)
where
    T: Sample + cpal::FromSample<f32>,
{
    for sample in output.iter_mut() {
        if let Some(value) = queue.pop() {
            *sample = T::from_sample(value);
        } else {
            *sample = T::from_sample(0.0);
        }
    }

    if !running.load(Ordering::Relaxed) {
        output.fill(T::from_sample(0.0));
    }
}

fn render_loop(
    engine: Arc<Mutex<HarmoniqEngine>>,
    config: BufferConfig,
    queue: Arc<ArrayQueue<f32>>,
    control_rx: Receiver<ControlMessage>,
    running: Arc<AtomicBool>,
    options: UltraLowLatencyOptions,
) -> anyhow::Result<()> {
    lock_memory()?;
    if let Some(priority) = options.realtime_priority {
        if let Err(err) = apply_realtime_priority(priority) {
            tracing::warn!(?err, "failed to apply realtime priority");
        }
    }

    let mut buffer = AudioBuffer::from_config(config.clone());
    let mut interleaved = vec![0.0f32; buffer.len() * config.layout.channels() as usize];

    while running.load(Ordering::Relaxed) {
        if let Ok(ControlMessage::Stop) = control_rx.try_recv() {
            break;
        }

        {
            let mut guard = engine.lock();
            guard
                .process_block(&mut buffer)
                .context("engine processing failed")?;
        }

        write_interleaved(&buffer, &mut interleaved);

        for &sample in &interleaved {
            while queue.push(sample).is_err() {
                let _ = queue.pop();
            }
        }
    }

    running.store(false, Ordering::Relaxed);
    Ok(())
}

fn write_interleaved(buffer: &AudioBuffer, target: &mut [f32]) {
    let channels = buffer.as_slice().len();
    let frames = buffer.len();
    for frame in 0..frames {
        for channel in 0..channels {
            target[frame * channels + channel] = buffer.as_slice()[channel][frame];
        }
    }
}

fn lock_memory() -> anyhow::Result<()> {
    unsafe {
        let flags = libc::MCL_CURRENT | libc::MCL_FUTURE;
        if libc::mlockall(flags) != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EPERM) {
                return Err(anyhow!("mlockall failed: {err}"));
            }
        }
    }
    Ok(())
}

fn apply_realtime_priority(priority: i32) -> anyhow::Result<()> {
    unsafe {
        let mut sched_param = libc::sched_param {
            sched_priority: priority,
        };
        let result =
            libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &mut sched_param);
        if result != 0 {
            return Err(anyhow!("failed to elevate thread priority: errno {result}"));
        }
    }
    Ok(())
}

pub fn alsa_devices_available() -> bool {
    const DEV_SND: &str = "/dev/snd";
    const PROC_ASOUND_CARDS: &str = "/proc/asound/cards";
    const PROC_ASOUND_PCM: &str = "/proc/asound/pcm";

    if Path::new(DEV_SND).is_dir() {
        if let Ok(mut entries) = fs::read_dir(DEV_SND) {
            if entries.any(|entry| entry.map(|_| ()).is_ok()) {
                return true;
            }
        }
    }

    if let Ok(cards) = fs::read_to_string(PROC_ASOUND_CARDS) {
        let trimmed = cards.trim();
        if !trimmed.is_empty() && !trimmed.contains("no soundcards") {
            return true;
        }
    }

    Path::new(PROC_ASOUND_PCM).exists()
}
