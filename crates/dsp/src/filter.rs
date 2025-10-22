use engine_graph::automation::ParameterId;
use engine_graph::{AudioNode, NodePreparation, PortBuffer, ProcessContext};
use simd::{load_f32x8, mul_add, store_f32x8, F32x8};

pub const PARAM_CUTOFF: ParameterId = 10;

pub struct OnePoleLowPass {
    cutoff: f32,
    alpha: f32,
    sample_rate: f32,
    state: Vec<f32>,
}

impl OnePoleLowPass {
    pub fn new(cutoff: f32) -> Self {
        Self {
            cutoff,
            alpha: 0.0,
            sample_rate: 48_000.0,
            state: Vec::new(),
        }
    }

    fn update_alpha(&mut self, cutoff: f32) {
        let omega = 2.0 * std::f32::consts::PI * cutoff / self.sample_rate.max(1.0);
        self.alpha = omega / (omega + 1.0);
    }
}

impl AudioNode for OnePoleLowPass {
    fn prepare(&mut self, preparation: &NodePreparation) {
        self.sample_rate = preparation.sample_rate;
        self.state = vec![0.0; preparation.channels];
        self.update_alpha(self.cutoff);
    }

    fn process(
        &mut self,
        inputs: &[PortBuffer],
        outputs: &mut [PortBuffer],
        context: &ProcessContext<'_>,
    ) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }
        let input = &inputs[0];
        let output = &mut outputs[0];
        let frames = context.frames.min(input.frames());
        let channels = input.channels().min(output.channels());
        let cutoff = context.parameters.value(PARAM_CUTOFF, self.cutoff);
        if (cutoff - self.cutoff).abs() > f32::EPSILON {
            self.cutoff = cutoff;
            self.update_alpha(cutoff);
        }
        let alpha = self.alpha;

        for channel in 0..channels {
            let mut state = self.state[channel];
            let input_channel = input.channel(channel);
            let output_channel = output.channel_mut(channel);
            let mut frame = 0;
            while frame + 8 <= frames {
                let x = load_f32x8(&input_channel[frame..]);
                let y_prev = F32x8::splat(state);
                let diff = x - y_prev;
                let y = mul_add(F32x8::splat(alpha), diff, y_prev);
                state = y.as_array()[7];
                store_f32x8(y, &mut output_channel[frame..]);
                frame += 8;
            }
            while frame < frames {
                state = state + alpha * (input_channel[frame] - state);
                output_channel[frame] = state;
                frame += 1;
            }
            self.state[channel] = state;
        }
    }
}
