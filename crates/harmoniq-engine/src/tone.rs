use crate::{AudioBuffer, BufferConfig};

/// Gentle tone-shaping filter applied to the engine's master bus to keep the
/// spectrum balanced. The design favours solid lows, relaxed mids, and crisp
/// highs without introducing heavy coloration or latency.
pub struct ToneShaper {
    low_state: Vec<f32>,
    high_state: Vec<f32>,
    low_alpha: f32,
    high_alpha: f32,
    low_gain: f32,
    mid_gain: f32,
    high_gain: f32,
}

impl ToneShaper {
    /// Constructs a new shaper tuned for the provided engine configuration.
    pub fn new(config: &BufferConfig) -> Self {
        let channels = config.layout.channels() as usize;
        let mut shaper = Self {
            low_state: vec![0.0; channels],
            high_state: vec![0.0; channels],
            low_alpha: smoothing_alpha(config.sample_rate, 120.0),
            high_alpha: smoothing_alpha(config.sample_rate, 6_000.0),
            low_gain: 1.12,
            mid_gain: 0.92,
            high_gain: 1.05,
        };

        shaper.ensure_state_len(channels);
        shaper
    }

    fn ensure_state_len(&mut self, channels: usize) {
        if self.low_state.len() != channels {
            self.low_state.resize(channels, 0.0);
        }
        if self.high_state.len() != channels {
            self.high_state.resize(channels, 0.0);
        }
    }

    /// Applies the tone-shaping curve to the provided buffer in-place.
    pub fn process(&mut self, buffer: &mut AudioBuffer) {
        let channels = buffer.channels().count();
        self.ensure_state_len(channels);

        for (channel_index, channel) in buffer.channels_mut().enumerate() {
            let mut low_state = self.low_state[channel_index];
            let mut high_state = self.high_state[channel_index];

            for sample in channel.iter_mut() {
                low_state += self.low_alpha * (*sample - low_state);
                let low = low_state;

                high_state += self.high_alpha * (*sample - high_state);
                let high = *sample - high_state;

                let mid = *sample - low - high;
                let shaped = low * self.low_gain + mid * self.mid_gain + high * self.high_gain;

                *sample = shaped.clamp(-1.0, 1.0);
            }

            self.low_state[channel_index] = low_state;
            self.high_state[channel_index] = high_state;
        }
    }
}

fn smoothing_alpha(sample_rate: f32, cutoff_hz: f32) -> f32 {
    if sample_rate <= 0.0 || cutoff_hz <= 0.0 {
        return 0.0;
    }
    let exponent = (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate).exp();
    1.0 - exponent
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_with_value(config: &BufferConfig, value: f32) -> AudioBuffer {
        let mut buffer = AudioBuffer::from_config(config.clone());
        for sample in buffer.iter_mut() {
            *sample = value;
        }
        buffer
    }

    #[test]
    fn boosts_low_content() {
        let config = BufferConfig::new(48_000.0, 128, crate::ChannelLayout::Stereo);
        let mut shaper = ToneShaper::new(&config);
        let mut buffer = buffer_with_value(&config, 0.25);
        shaper.process(&mut buffer);

        // After the filter settles the steady-state value should be close to the
        // low gain factor applied to the constant signal.
        let left = buffer.as_slice()[0][127];
        assert!(left > 0.25, "expected low end boost");
    }
}
