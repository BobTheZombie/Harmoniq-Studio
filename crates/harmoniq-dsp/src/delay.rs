/// Stereo feedback delay line operating without runtime allocations.
#[derive(Clone, Debug)]
pub struct StereoDelay {
    left: Vec<f32>,
    right: Vec<f32>,
    write: usize,
    sample_rate: f32,
    max_samples: usize,
    delay_samples: usize,
    time_seconds: f32,
    feedback: f32,
    mix: f32,
}

impl StereoDelay {
    #[inline]
    pub fn new(sample_rate: f32, max_delay_s: f32) -> Self {
        let mut delay = Self {
            left: Vec::new(),
            right: Vec::new(),
            write: 0,
            sample_rate: sample_rate.max(1.0),
            max_samples: 0,
            delay_samples: 1,
            time_seconds: 0.25,
            feedback: 0.35,
            mix: 0.3,
        };
        delay.prepare(sample_rate, max_delay_s);
        delay
    }

    #[inline]
    pub fn prepare(&mut self, sample_rate: f32, max_delay_s: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let max_samples = (self.sample_rate * max_delay_s.max(0.001)).ceil() as usize;
        let max_samples = max_samples.max(1);
        if self.max_samples != max_samples {
            self.left.resize(max_samples, 0.0);
            self.right.resize(max_samples, 0.0);
            self.max_samples = max_samples;
            self.write = 0;
        }
        self.clear();
        self.apply_time_seconds();
    }

    #[inline]
    pub fn set_time_seconds(&mut self, seconds: f32) {
        self.time_seconds = seconds.max(0.0);
        self.apply_time_seconds();
    }

    #[inline]
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(-0.995, 0.995);
    }

    #[inline]
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    #[inline]
    pub fn process_sample(&mut self, input_l: f32, input_r: f32) -> (f32, f32) {
        let len = self.max_samples;
        let delay = self.delay_samples.min(len.saturating_sub(1));
        let read = if self.write >= delay {
            self.write - delay
        } else {
            len + self.write - delay
        } % len;
        let wet_l = self.left[read];
        let wet_r = self.right[read];
        let dry = 1.0 - self.mix;
        let out_l = dry * input_l + self.mix * wet_l;
        let out_r = dry * input_r + self.mix * wet_r;
        self.left[self.write] = input_l + wet_l * self.feedback;
        self.right[self.write] = input_r + wet_r * self.feedback;
        self.write += 1;
        if self.write >= len {
            self.write = 0;
        }
        (out_l, out_r)
    }

    fn apply_time_seconds(&mut self) {
        let max_time = self.max_samples as f32 / self.sample_rate;
        let seconds = self.time_seconds.min(max_time);
        let samples = (seconds * self.sample_rate).round() as usize;
        self.delay_samples = samples.clamp(1, self.max_samples);
    }

    #[inline]
    pub fn clear(&mut self) {
        for sample in &mut self.left {
            *sample = 0.0;
        }
        for sample in &mut self.right {
            *sample = 0.0;
        }
        self.write = 0;
    }
}
