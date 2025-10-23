//! Abstraction over platform audio backends.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use harmoniq_utils::time::TempoInfo;
use parking_lot::Mutex;
use tracing::{debug, error};

/// Audio configuration describing the realtime stream.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Sample rate in Hertz.
    pub sample_rate: u32,
    /// Number of frames per processing block.
    pub block_size: usize,
    /// Number of audio channels.
    pub channels: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            block_size: 256,
            channels: 2,
        }
    }
}

/// Wrapper around the currently selected audio device.
#[derive(Debug)]
pub struct AudioIo {
    config: AudioConfig,
    state: Arc<AudioState>,
}

impl AudioIo {
    /// Creates a new audio interface abstraction.
    pub fn new(config: Option<AudioConfig>) -> anyhow::Result<Self> {
        let config = config.unwrap_or_default();
        let state = Arc::new(AudioState::new(config.clone()));
        Ok(Self { config, state })
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &AudioConfig {
        &self.config
    }

    /// Spawns a realtime thread invoking the provided callback.
    pub fn start_stream<F>(&self, mut callback: F) -> anyhow::Result<AudioStreamHandle>
    where
        F: FnMut(&mut AudioBlock) + Send + 'static,
    {
        let state = self.state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let config = self.config.clone();
        let handle = thread::Builder::new()
            .name("harmoniq-audio".into())
            .spawn(move || {
                let mut block = AudioBlock::new(config.channels, config.block_size);
                while !stop_clone.load(Ordering::Relaxed) {
                    callback(&mut block);
                    state.publish_block(&block);
                    let sleep = Duration::from_secs_f32(
                        config.block_size as f32 / config.sample_rate as f32,
                    );
                    thread::sleep(sleep);
                }
            })
            .context("failed to spawn audio thread")?;
        Ok(AudioStreamHandle {
            handle: Some(handle),
            stop,
        })
    }

    /// Retrieves the most recently rendered block, suitable for metering.
    pub fn latest_block(&self) -> Option<AudioBlock> {
        self.state.latest_block()
    }
}

/// Persistent state shared with the UI for metering.
#[derive(Debug)]
struct AudioState {
    _config: AudioConfig,
    latest: Mutex<Option<AudioBlock>>,
}

impl AudioState {
    fn new(config: AudioConfig) -> Self {
        Self {
            _config: config,
            latest: Mutex::new(None),
        }
    }

    fn publish_block(&self, block: &AudioBlock) {
        *self.latest.lock() = Some(block.clone());
    }

    fn latest_block(&self) -> Option<AudioBlock> {
        self.latest.lock().clone()
    }
}

/// Handle to a running audio stream.
pub struct AudioStreamHandle {
    handle: Option<thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl Drop for AudioStreamHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            if let Err(err) = handle.join() {
                error!(?err, "failed to join audio thread");
            }
        }
    }
}

/// Audio buffer handed to the engine each callback.
#[derive(Debug, Clone)]
pub struct AudioBlock {
    /// Interleaved audio samples per channel.
    pub channels: Vec<Vec<f32>>,
    /// Number of frames.
    pub frames: usize,
}

impl AudioBlock {
    /// Creates a silent audio block.
    pub fn new(channels: usize, frames: usize) -> Self {
        Self {
            channels: vec![vec![0.0; frames]; channels],
            frames,
        }
    }

    /// Zeros the buffer.
    #[inline(always)]
    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            for sample in channel.iter_mut() {
                *sample = 0.0;
            }
        }
    }
}

/// Describes the transport clock shared with the UI.
#[derive(Debug, Clone, Copy)]
pub struct TransportState {
    /// Current tempo context.
    pub tempo: TempoInfo,
    /// Current playhead position in samples.
    pub position: u64,
    /// Whether playback is active.
    pub playing: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            tempo: TempoInfo::default(),
            position: 0,
            playing: false,
        }
    }
}

/// Enumeration describing available backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Virtual backend used for testing and development.
    Virtual,
    /// Placeholder for a future PipeWire backend.
    PipeWire,
}

/// High level description of a device.
#[derive(Debug, Clone)]
pub struct DeviceDescriptor {
    /// Human readable name.
    pub name: String,
    /// Number of output channels supported.
    pub output_channels: usize,
    /// Preferred sample rate.
    pub preferred_sample_rate: u32,
}

impl DeviceDescriptor {
    /// Creates a descriptor for the default virtual device.
    pub fn virtual_device() -> Self {
        Self {
            name: "Virtual Device".into(),
            output_channels: 2,
            preferred_sample_rate: 48_000,
        }
    }
}

/// Enumerates available audio devices.
pub fn enumerate_devices() -> Vec<DeviceDescriptor> {
    debug!("enumerating virtual audio device list");
    vec![DeviceDescriptor::virtual_device()]
}
