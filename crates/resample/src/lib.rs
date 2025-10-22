use simd::F32x8;

pub struct LinearResampler {
    ratio: f32,
    phase: f32,
}

impl LinearResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            ratio: input_rate as f32 / output_rate as f32,
            phase: 0.0,
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> usize {
        let mut input_index = self.phase;
        let mut produced = 0;
        let ratio = self.ratio;
        let max_index = input.len().saturating_sub(1) as f32;
        while produced + 8 <= output.len() {
            let mut samples = [0.0f32; 8];
            for lane in 0..8 {
                let index_floor = input_index.floor();
                let frac = input_index - index_floor;
                let idx = index_floor as usize;
                if idx + 1 >= input.len() {
                    samples[lane] = input[idx];
                } else {
                    samples[lane] = input[idx] * (1.0 - frac) + input[idx + 1] * frac;
                }
                input_index = (input_index + ratio).min(max_index);
            }
            let simd = F32x8::from_array(samples);
            simd.write_to_slice(&mut output[produced..produced + 8]);
            produced += 8;
        }
        while produced < output.len() {
            let index_floor = input_index.floor();
            let frac = input_index - index_floor;
            let idx = index_floor as usize;
            if idx + 1 >= input.len() {
                output[produced] = input[idx];
            } else {
                output[produced] = input[idx] * (1.0 - frac) + input[idx + 1] * frac;
            }
            produced += 1;
            input_index = (input_index + ratio).min(max_index);
        }
        self.phase = input_index - input_index.floor();
        produced
    }
}
