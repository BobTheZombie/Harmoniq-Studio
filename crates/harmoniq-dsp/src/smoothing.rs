/// One-pole smoothing filter suitable for real-time parameter transitions.
#[derive(Clone, Copy, Debug)]
pub struct OnePole {
    coeff: f32,
    state: f32,
}

impl OnePole {
    #[inline]
    pub fn new(sample_rate: f32, time_ms: f32) -> Self {
        let mut s = Self {
            coeff: 0.0,
            state: 0.0,
        };
        s.set_time_ms(sample_rate, time_ms);
        s
    }

    #[inline]
    pub fn set_time_ms(&mut self, sample_rate: f32, time_ms: f32) {
        let rate = sample_rate.max(1.0);
        let time = time_ms.max(0.01) * 0.001;
        let tau = time * rate;
        let coeff = if tau <= 1.0 {
            1.0
        } else {
            1.0 - (-1.0 / tau).exp()
        };
        self.coeff = coeff.clamp(0.0, 1.0);
    }

    #[inline]
    pub fn reset(&mut self, value: f32) {
        self.state = value;
    }

    #[inline]
    pub fn next(&mut self, target: f32) -> f32 {
        self.state += self.coeff * (target - self.state);
        self.state
    }

    #[inline]
    pub fn state(&self) -> f32 {
        self.state
    }
}
