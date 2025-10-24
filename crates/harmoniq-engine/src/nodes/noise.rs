use crate::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};

/// White noise source backed by a linear congruential generator.
#[derive(Debug, Clone, Copy)]
pub struct NoiseNode {
    seed: u64,
    amplitude: f32,
}

impl NoiseNode {
    pub fn new(amplitude: f32) -> Self {
        Self {
            seed: 0xDEADBEEFCAFEBABE,
            amplitude: amplitude.abs().min(1.0),
        }
    }

    #[inline]
    fn next_sample(&mut self) -> f32 {
        // 64-bit LCG parameters from Numerical Recipes
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let bits = (self.seed >> 41) as u32;
        let value = (bits as f32) / (1u32 << 23) as f32;
        (value * 2.0 - 1.0) * self.amplitude
    }
}

impl AudioProcessor for NoiseNode {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.noise", "Noise", "Harmoniq Labs")
            .with_description("Zero-allocation noise source for default graphs")
    }

    fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let frames = buffer.len();
        if frames == 0 {
            return Ok(());
        }
        let channels = buffer.as_mut_slice();
        let channel_count = channels.len();
        for frame in 0..frames {
            let sample = self.next_sample();
            for channel in channels.iter_mut().take(channel_count) {
                if frame < channel.len() {
                    channel[frame] = sample;
                }
            }
        }
        Ok(())
    }

    fn supports_layout(&self, _layout: ChannelLayout) -> bool {
        true
    }
}
