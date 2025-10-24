use crate::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};

/// Simple gain processor used by the built-in demo graph.
#[derive(Clone, Copy, Debug)]
pub struct GainNode {
    gain: f32,
}

impl GainNode {
    pub fn new(gain: f32) -> Self {
        Self { gain }
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }
}

impl AudioProcessor for GainNode {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.gain", "Gain", "Harmoniq Labs")
            .with_description("Linear gain stage for built-in graphs")
    }

    fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let gain = self.gain;
        if (gain - 1.0).abs() < f32::EPSILON {
            return Ok(());
        }

        for sample in buffer.iter_mut() {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
        Ok(())
    }

    fn supports_layout(&self, _layout: ChannelLayout) -> bool {
        true
    }
}
