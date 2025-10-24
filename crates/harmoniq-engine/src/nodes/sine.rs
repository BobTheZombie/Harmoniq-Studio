use std::f32::consts::TAU;

use crate::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};

/// A minimal sine oscillator implementing [`AudioProcessor`].
///
/// This node is intended for quick smoke tests and demonstrations of the
/// real-time engine. It keeps internal state minimal and produces a continuous
/// sine wave at the configured frequency.
///
/// # Examples
/// ```no_run
/// use harmoniq_engine::{
///     BufferConfig, ChannelLayout, GraphBuilder, HarmoniqEngine, NodeOsc,
///     start_realtime,
/// };
/// use std::error::Error;
/// use std::thread;
/// use std::time::Duration;
///
/// fn main() -> Result<(), Box<dyn Error>> {
///     let config = BufferConfig::new(48_000.0, 256, ChannelLayout::Stereo);
///     let mut engine = HarmoniqEngine::new(config.clone())?;
///
///     let sine = engine.register_processor(Box::new(NodeOsc::new(440.0).with_amplitude(0.2)))?;
///     let mut graph = GraphBuilder::new();
///     let node = graph.add_node(sine);
///     graph.connect_to_mixer(node, 1.0)?;
///     engine.replace_graph(graph.build())?;
///
///     let (stream, handle) = start_realtime(engine)?;
///     thread::sleep(Duration::from_millis(200));
///
///     drop(stream);
///     drop(handle);
///     Ok(())
/// }
/// ```
pub struct NodeOsc {
    frequency: f32,
    amplitude: f32,
    phase: f32,
    phase_delta: f32,
    sample_rate: f32,
}

impl NodeOsc {
    /// Creates a new oscillator running at the provided frequency in hertz.
    pub fn new(frequency: f32) -> Self {
        Self {
            frequency: frequency.max(0.0),
            amplitude: 1.0,
            phase: 0.0,
            phase_delta: 0.0,
            sample_rate: 44_100.0,
        }
    }

    /// Sets the oscillator amplitude.
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude;
        self
    }

    /// Adjusts the oscillator frequency.
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency.max(0.0);
        self.update_phase_delta();
    }

    fn update_phase_delta(&mut self) {
        if self.sample_rate > 0.0 {
            self.phase_delta = (TAU * self.frequency) / self.sample_rate;
        } else {
            self.phase_delta = 0.0;
        }
    }
}

impl AudioProcessor for NodeOsc {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.sine", "Sine", "Harmoniq Labs")
            .with_description("Simple sine oscillator for testing the audio engine")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate.max(f32::EPSILON);
        self.update_phase_delta();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let frames = buffer.len();
        if frames == 0 {
            return Ok(());
        }

        let amplitude = self.amplitude;
        let mut phase = self.phase;
        let phase_delta = self.phase_delta;
        let channel_count = buffer.channel_count();
        let data = buffer.as_mut_slice();

        for frame in 0..frames {
            let value = (phase).sin() * amplitude;
            for channel in 0..channel_count {
                let index = channel * frames + frame;
                if index < data.len() {
                    data[index] = value;
                }
            }
            phase = (phase + phase_delta).rem_euclid(TAU);
        }

        self.phase = phase;
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

/// Backwards compatible alias for the previous name.
pub type SineNode = NodeOsc;
