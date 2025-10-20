use harmoniq_engine::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};

/// Simple gain plugin for balancing levels in the mixer.
#[derive(Debug, Clone)]
pub struct GainPlugin {
    gain: f32,
}

impl GainPlugin {
    pub fn new(gain: f32) -> Self {
        Self { gain }
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }
}

impl Default for GainPlugin {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl AudioProcessor for GainPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.gain", "Gain", "Harmoniq Labs")
    }

    fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for sample in buffer.iter_mut() {
            *sample *= self.gain;
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}
