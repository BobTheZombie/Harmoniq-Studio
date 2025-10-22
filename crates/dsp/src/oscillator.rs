use engine_graph::automation::ParameterId;
use engine_graph::{AudioNode, NodePreparation, PortBuffer, ProcessContext};
use std::f32::consts::TAU;

pub const PARAM_FREQUENCY: ParameterId = 1;
pub const PARAM_AMPLITUDE: ParameterId = 2;

pub struct SineOscillator {
    frequency: f32,
    amplitude: f32,
    phase: f32,
    sample_rate: f32,
}

impl SineOscillator {
    pub fn new(frequency: f32, amplitude: f32) -> Self {
        Self {
            frequency,
            amplitude,
            phase: 0.0,
            sample_rate: 48_000.0,
        }
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude;
    }
}

impl AudioNode for SineOscillator {
    fn prepare(&mut self, preparation: &NodePreparation) {
        self.sample_rate = preparation.sample_rate;
        self.phase = 0.0;
    }

    fn process(
        &mut self,
        _inputs: &[PortBuffer],
        outputs: &mut [PortBuffer],
        context: &ProcessContext<'_>,
    ) {
        if outputs.is_empty() {
            return;
        }
        let output = &mut outputs[0];
        let frames = context.frames.min(output.frames());
        let channels = output.channels();
        let mut phase = self.phase;
        let frequency = context.parameters.value(PARAM_FREQUENCY, self.frequency);
        let amplitude = context.parameters.value(PARAM_AMPLITUDE, self.amplitude);
        let phase_increment = frequency / self.sample_rate;

        for frame in 0..frames {
            let sample = (phase * TAU).sin() * amplitude;
            phase += phase_increment;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            for channel in 0..channels {
                output.channel_mut(channel)[frame] = sample;
            }
        }

        self.phase = phase;
    }
}
