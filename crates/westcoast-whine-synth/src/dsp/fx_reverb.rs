pub struct PlateReverb {
    comb_buffers: [Vec<f32>; 4],
    comb_indices: [usize; 4],
    comb_feedback: f32,
    allpass_buffers: [Vec<f32>; 2],
    allpass_indices: [usize; 2],
    sample_rate: f32,
}

impl PlateReverb {
    pub fn new() -> Self {
        Self {
            comb_buffers: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            comb_indices: [0; 4],
            comb_feedback: 0.7,
            allpass_buffers: [Vec::new(), Vec::new()],
            allpass_indices: [0; 2],
            sample_rate: 48_000.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let comb_times = [0.0297, 0.0371, 0.0411, 0.0437];
        for (buffer, &time) in self.comb_buffers.iter_mut().zip(comb_times.iter()) {
            let len = (time * self.sample_rate) as usize + 1;
            if buffer.len() != len {
                *buffer = vec![0.0; len];
            } else {
                buffer.fill(0.0);
            }
        }
        self.comb_indices = [0; 4];

        let allpass_times = [0.005, 0.0017];
        for (buffer, &time) in self.allpass_buffers.iter_mut().zip(allpass_times.iter()) {
            let len = (time * self.sample_rate) as usize + 1;
            if buffer.len() != len {
                *buffer = vec![0.0; len];
            } else {
                buffer.fill(0.0);
            }
        }
        self.allpass_indices = [0; 2];
    }

    pub fn process(&mut self, left: f32, right: f32, mix: f32) -> (f32, f32) {
        if self.comb_buffers[0].is_empty() {
            return (left, right);
        }

        let input = 0.5 * (left + right);
        let mut comb_sum = 0.0;
        for i in 0..self.comb_buffers.len() {
            let buffer = &mut self.comb_buffers[i];
            let idx = self.comb_indices[i];
            let out = buffer[idx];
            buffer[idx] = input + out * self.comb_feedback;
            self.comb_indices[i] = (idx + 1) % buffer.len();
            comb_sum += out;
        }
        let mut y = comb_sum / self.comb_buffers.len() as f32;

        let allpass_feedback = 0.5;
        for i in 0..self.allpass_buffers.len() {
            let buffer = &mut self.allpass_buffers[i];
            let idx = self.allpass_indices[i];
            let buf_out = buffer[idx];
            let new = y + (-allpass_feedback) * buf_out;
            buffer[idx] = new;
            self.allpass_indices[i] = (idx + 1) % buffer.len();
            y = buf_out + new * allpass_feedback;
        }

        let wet = y;
        let mix = mix.clamp(0.0, 1.0);
        (
            left * (1.0 - mix) + wet * mix,
            right * (1.0 - mix) + wet * mix,
        )
    }
}
