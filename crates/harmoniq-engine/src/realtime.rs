use std::cmp;
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, Sample, SampleFormat, SampleRate, Stream, StreamConfig};
use crossbeam::queue::ArrayQueue;
use parking_lot::Mutex;

use crate::{AudioBuffer, BufferConfig, EngineCommandQueue, HarmoniqEngine};

const DEFAULT_QUEUE_DEPTH: usize = 3;

/// Handle returned by [`start_realtime`] for controlling the running engine.
pub struct EngineHandle {
    engine: Arc<Mutex<HarmoniqEngine>>,
    running: Arc<AtomicBool>,
    queue: Arc<ArrayQueue<f32>>,
    render_thread: Option<JoinHandle<()>>,
    config: BufferConfig,
}

impl EngineHandle {
    /// Returns the engine configuration currently driving the stream.
    pub fn config(&self) -> &BufferConfig {
        &self.config
    }

    /// Retrieves the engine's command queue for issuing transport and graph updates.
    pub fn command_queue(&self) -> EngineCommandQueue {
        self.engine.lock().command_queue()
    }

    /// Provides a shared reference to the underlying [`HarmoniqEngine`].
    pub fn engine(&self) -> Arc<Mutex<HarmoniqEngine>> {
        Arc::clone(&self.engine)
    }

    /// Executes a closure with exclusive access to the engine state.
    pub fn with_engine<F, R>(&self, func: F) -> R
    where
        F: FnOnce(&mut HarmoniqEngine) -> R,
    {
        let mut engine = self.engine.lock();
        func(&mut *engine)
    }

    /// Number of interleaved samples currently buffered for the audio callback.
    pub fn buffered_samples(&self) -> usize {
        self.queue.len()
    }

    /// Indicates whether the render thread is still active.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Stops the render thread and waits for it to finish.
    pub fn shutdown(mut self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.render_thread.take() {
            handle
                .join()
                .map_err(|_| anyhow!("realtime render thread panicked"))?;
        }
        Ok(())
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.render_thread.take() {
            if let Err(err) = handle.join() {
                tracing::error!(?err, "failed to join realtime render thread");
            }
        }
    }
}

/// Starts a CPAL output stream that pulls audio from the Harmoniq engine.
///
/// A dedicated render thread performs the heavy lifting while the CPAL callback
/// stays allocation and lock free.
pub fn start_realtime(engine: HarmoniqEngine) -> anyhow::Result<(Stream, EngineHandle)> {
    let config = engine.config().clone();
    let engine = Arc::new(Mutex::new(engine));

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("failed to acquire default output device")?;
    let supported = choose_stream_config(&device, &config)?;
    let mut stream_config: StreamConfig = supported.config();

    let desired_channels = config.layout.channels() as usize;
    let output_channels = stream_config.channels as usize;
    if output_channels != desired_channels {
        tracing::warn!(
            device_channels = output_channels,
            engine_channels = desired_channels,
            "Device channel count differs from engine configuration; extra channels will be silent"
        );
    }

    let frames_per_block = cmp::max(1, config.block_size);
    let buffer_frames = u32::try_from(frames_per_block).unwrap_or(u32::MAX);
    stream_config.buffer_size = BufferSize::Fixed(buffer_frames);

    let queue_capacity = frames_per_block
        .saturating_mul(output_channels.max(1))
        .saturating_mul(DEFAULT_QUEUE_DEPTH.max(2))
        .max(output_channels.max(1));
    let queue = Arc::new(ArrayQueue::new(queue_capacity));
    prefill_queue(
        &queue,
        frames_per_block.saturating_mul(output_channels.max(1)),
    );

    let running = Arc::new(AtomicBool::new(true));

    let render_thread = spawn_render_thread(
        Arc::clone(&engine),
        Arc::clone(&queue),
        Arc::clone(&running),
        config.clone(),
        output_channels,
    )?;

    let stream = build_stream(
        &device,
        &stream_config,
        supported.sample_format(),
        Arc::clone(&queue),
        Arc::clone(&running),
    )?;
    stream.play()?;

    let handle = EngineHandle {
        engine,
        running,
        queue,
        render_thread: Some(render_thread),
        config,
    };

    Ok((stream, handle))
}

fn spawn_render_thread(
    engine: Arc<Mutex<HarmoniqEngine>>,
    queue: Arc<ArrayQueue<f32>>,
    running: Arc<AtomicBool>,
    config: BufferConfig,
    output_channels: usize,
) -> anyhow::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("harmoniq-realtime-render".into())
        .spawn(move || {
            ensure_denormals_disabled();
            let mut buffer = AudioBuffer::from_config(config.clone());
            let stride = output_channels.max(1);
            let mut interleaved = vec![0.0f32; stride.saturating_mul(cmp::max(1, buffer.len()))];

            while running.load(Ordering::Relaxed) {
                let process_result = {
                    let mut guard = engine.lock();
                    guard.process_block(&mut buffer)
                };

                if let Err(err) = process_result {
                    tracing::error!(?err, "engine processing failed in realtime thread");
                    buffer.clear();
                }

                let frames = buffer.len();
                if frames == 0 {
                    continue;
                }

                let required = frames.saturating_mul(stride);
                if interleaved.len() < required {
                    interleaved.resize(required, 0.0);
                }

                let written =
                    interleave_buffer(&buffer, output_channels, &mut interleaved[..required]);

                for sample in interleaved[..written].iter().copied() {
                    if !push_sample(&queue, sample, &running) {
                        return;
                    }
                }
            }
        })
        .context("failed to spawn realtime render thread")
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    format: SampleFormat,
    queue: Arc<ArrayQueue<f32>>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<Stream> {
    match format {
        SampleFormat::F32 => build_output_stream::<f32>(device, config, queue, running),
        SampleFormat::I16 => build_output_stream::<i16>(device, config, queue, running),
        SampleFormat::U16 => build_output_stream::<u16>(device, config, queue, running),
        other => Err(anyhow!("unsupported sample format: {other:?}")),
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    queue: Arc<ArrayQueue<f32>>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<Stream>
where
    T: Sample + cpal::SizedSample + FromSample<f32> + Send + 'static,
{
    let silence = T::from_sample(0.0f32);
    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _info| {
            ensure_denormals_disabled();
            if !running.load(Ordering::Relaxed) {
                for sample in output.iter_mut() {
                    *sample = silence;
                }
                return;
            }

            for sample in output.iter_mut() {
                if let Some(value) = queue.pop() {
                    *sample = T::from_sample(value);
                } else {
                    *sample = silence;
                }
            }
        },
        move |err| {
            tracing::error!(?err, "cpal output stream error");
        },
        None,
    )?;
    Ok(stream)
}

fn choose_stream_config(
    device: &cpal::Device,
    config: &BufferConfig,
) -> anyhow::Result<cpal::SupportedStreamConfig> {
    let desired_channels = cmp::max(1, config.layout.channels() as usize) as u16;
    let rate_hz = config.sample_rate.max(1.0).round();
    let clamped = rate_hz.clamp(1.0, u32::MAX as f32) as u32;
    let desired_rate = SampleRate(clamped);

    if let Ok(configs) = device.supported_output_configs() {
        for range in configs {
            if range.channels() == desired_channels
                && range.sample_format() == SampleFormat::F32
                && range.min_sample_rate() <= desired_rate
                && desired_rate <= range.max_sample_rate()
            {
                return Ok(range.with_sample_rate(desired_rate));
            }
        }
    }

    if let Ok(configs) = device.supported_output_configs() {
        for range in configs {
            if range.channels() == desired_channels && range.sample_format() == SampleFormat::F32 {
                return Ok(range.with_sample_rate(range.max_sample_rate()));
            }
        }
    }

    if let Ok(configs) = device.supported_output_configs() {
        for range in configs {
            if range.channels() == desired_channels {
                return Ok(range.with_sample_rate(range.max_sample_rate()));
            }
        }
    }

    device
        .default_output_config()
        .context("failed to fetch default output config")
}

fn interleave_buffer(buffer: &AudioBuffer, output_channels: usize, target: &mut [f32]) -> usize {
    let channels = buffer.as_slice();
    let frames = buffer.len();
    let channel_count = channels.len();
    let mut index = 0;

    for frame in 0..frames {
        for channel in 0..output_channels {
            if index >= target.len() {
                return index;
            }

            let value = if channel < channel_count {
                channels[channel][frame]
            } else {
                0.0
            };
            target[index] = value;
            index += 1;
        }
    }

    index
}

fn push_sample(queue: &ArrayQueue<f32>, mut value: f32, running: &AtomicBool) -> bool {
    while running.load(Ordering::Relaxed) {
        match queue.push(value) {
            Ok(()) => return true,
            Err(returned) => {
                value = returned;
                thread::yield_now();
            }
        }
    }
    false
}

fn prefill_queue(queue: &ArrayQueue<f32>, samples: usize) {
    let capacity = queue.capacity();
    if capacity == 0 {
        return;
    }
    let fill = samples.min(capacity);
    for _ in 0..fill {
        let _ = queue.push(0.0);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
thread_local! {
    static DENORMAL_STATE: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

fn ensure_denormals_disabled() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        DENORMAL_STATE.with(|state| {
            if !state.get() {
                unsafe {
                    use std::arch::asm;

                    const FTZ: u32 = 1 << 15;
                    const DAZ: u32 = 1 << 6;
                    let mut csr: u32 = 0;
                    asm!(
                        "stmxcsr [{ptr}]",
                        ptr = in(reg) &mut csr,
                        options(nostack, preserves_flags),
                    );
                    csr |= FTZ | DAZ;
                    asm!(
                        "ldmxcsr [{ptr}]",
                        ptr = in(reg) &csr,
                        options(nostack, preserves_flags),
                    );
                }
                state.set(true);
            }
        });
    }
}
