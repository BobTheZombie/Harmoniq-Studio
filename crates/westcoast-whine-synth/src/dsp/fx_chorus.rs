use std::f32::consts::TAU;

pub struct StereoChorus {
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    write_index: usize,
    phase: f32,
    sample_rate: f32,
    max_delay_samples: usize,
}

impl StereoChorus {
    pub fn new() -> Self {
        Self {
            delay_l: Vec::new(),
            delay_r: Vec::new(),
            write_index: 0,
            phase: 0.0,
            sample_rate: 48_000.0,
            max_delay_samples: 1,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.max_delay_samples = (self.sample_rate * 0.05) as usize + 2;
        if self.delay_l.len() != self.max_delay_samples {
            self.delay_l = vec![0.0; self.max_delay_samples];
            self.delay_r = vec![0.0; self.max_delay_samples];
        } else {
            self.delay_l.fill(0.0);
            self.delay_r.fill(0.0);
        }
        self.write_index = 0;
        self.phase = 0.0;
    }

    #[inline]
    fn read_delay(buffer: &[f32], index: usize, delay_samples: f32) -> f32 {
        let len = buffer.len();
        if len == 0 {
            return 0.0;
        }

        let read_pos = index as f32 - delay_samples;
        let base_index = read_pos.floor();
        let frac = (read_pos - base_index).clamp(0.0, 1.0);
        let index0 = (base_index as isize).rem_euclid(len as isize) as usize;
        let index1 = (index0 + 1) % len;
        let sample0 = buffer[index0];
        let sample1 = buffer[index1];
        sample0 + (sample1 - sample0) * frac
    }

    pub fn process(
        &mut self,
        left: f32,
        right: f32,
        rate: f32,
        depth: f32,
        mix: f32,
    ) -> (f32, f32) {
        if self.delay_l.is_empty() {
            return (left, right);
        }

        let base_delay = 0.015 * self.sample_rate;
        let mod_depth = (0.01 * self.sample_rate) * depth.clamp(0.0, 1.0);
        let lfo = self.phase.sin();
        let l_delay =
            (base_delay + mod_depth * lfo).clamp(1.0, (self.max_delay_samples - 2) as f32);
        let r_delay =
            (base_delay + mod_depth * -lfo).clamp(1.0, (self.max_delay_samples - 2) as f32);

        let delayed_l = Self::read_delay(&self.delay_l, self.write_index, l_delay);
        let delayed_r = Self::read_delay(&self.delay_r, self.write_index, r_delay);

        self.delay_l[self.write_index] = left + delayed_l * 0.2;
        self.delay_r[self.write_index] = right + delayed_r * 0.2;

        self.write_index += 1;
        if self.write_index >= self.max_delay_samples {
            self.write_index = 0;
        }

        self.phase += (rate / self.sample_rate).max(0.0) * TAU;
        if self.phase > TAU {
            self.phase -= TAU;
        }

        let wet_l = delayed_l;
        let wet_r = delayed_r;
        let mix = mix.clamp(0.0, 1.0);

        (
            left * (1.0 - mix) + wet_l * mix,
            right * (1.0 - mix) + wet_r * mix,
        )
    }
}
