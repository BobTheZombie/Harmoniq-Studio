//! Plugin hosting abstraction covering VST3, CLAP, and Harmoniq plugins.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use harmoniq_plugin_sdk::HqPluginDescriptor;
use harmoniq_utils::rt::{rt_queue, RtReceiver, RtSender};
use tracing::debug;
use uuid::Uuid;

/// Identifier returned when loading a plugin instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(Uuid);

impl Handle {
    fn new() -> Self {
        Handle(Uuid::new_v4())
    }
}

/// Identifier for a plugin parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamId(pub u32);

/// Audio buffer passed to plugins.
#[derive(Debug)]
pub struct AudioBus {
    /// Per channel sample buffers.
    pub channels: Vec<Vec<f32>>,
}

impl AudioBus {
    /// Creates a new audio bus with the given channel count and block size.
    pub fn new(channels: usize, frames: usize) -> Self {
        Self {
            channels: vec![vec![0.0; frames]; channels],
        }
    }
}

/// Container for buffered MIDI events.
#[derive(Debug, Default)]
pub struct MidiBuffer {
    /// Raw MIDI bytes and sample offsets.
    pub events: Vec<(u32, [u8; 3])>,
}

/// Trait implemented by concrete plugin host backends.
pub trait PluginHost {
    /// Loads the plugin at the given path.
    fn load(&mut self, path: &Path) -> Result<Handle>;
    /// Updates the sample rate.
    fn set_sample_rate(&mut self, hz: f32);
    /// Updates the block size.
    fn set_block_size(&mut self, n: usize);
    /// Processes audio for a particular instance.
    fn process(&mut self, handle: Handle, audio: &mut AudioBus, midi: &mut MidiBuffer);
    /// Sets the value of a parameter.
    fn set_param(&mut self, handle: Handle, id: ParamId, value: f32);
    /// Retrieves the plugin latency.
    fn latency_samples(&self, handle: Handle) -> u32;
}

/// Minimal in-process host implementation used for testing.
#[derive(Default)]
pub struct NullHost {
    sample_rate: f32,
    block_size: usize,
    instances: HashMap<Handle, PluginStub>,
}

impl NullHost {
    /// Creates a new host instance.
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            block_size: 256,
            instances: HashMap::new(),
        }
    }
}

impl PluginHost for NullHost {
    fn load(&mut self, _path: &Path) -> Result<Handle> {
        let handle = Handle::new();
        let stub = PluginStub {
            descriptor: HqPluginDescriptor::stub(),
            latency: 0,
        };
        self.instances.insert(handle, stub);
        Ok(handle)
    }

    fn set_sample_rate(&mut self, hz: f32) {
        self.sample_rate = hz;
    }

    fn set_block_size(&mut self, n: usize) {
        self.block_size = n;
    }

    fn process(&mut self, handle: Handle, audio: &mut AudioBus, _midi: &mut MidiBuffer) {
        if let Some(instance) = self.instances.get(&handle) {
            debug!(?instance.descriptor.name, "processing stub plugin");
        }
        for channel in &mut audio.channels {
            for sample in channel.iter_mut() {
                *sample *= 1.0;
            }
        }
    }

    fn set_param(&mut self, _handle: Handle, _id: ParamId, _value: f32) {
        // Stub implementation.
    }

    fn latency_samples(&self, handle: Handle) -> u32 {
        self.instances
            .get(&handle)
            .map(|instance| instance.latency)
            .unwrap_or_default()
    }
}

/// Internal representation of a plugin instance.
#[derive(Debug, Clone)]
struct PluginStub {
    descriptor: HqPluginDescriptor,
    latency: u32,
}

/// Message used by the sandbox bridge.
#[derive(Debug)]
pub enum SandboxMessage {
    /// Loads a plugin in the sandbox process.
    Load { path: String },
    /// Process request.
    Process { handle: Handle },
}

/// Bridge channel pair used for sandboxing.
pub fn sandbox_channels(capacity: usize) -> (RtSender<SandboxMessage>, RtReceiver<SandboxMessage>) {
    rt_queue(capacity)
}
