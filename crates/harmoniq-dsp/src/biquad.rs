/// Simple state-variable filter for low-pass processing.
#[derive(Clone, Copy, Debug)]
pub struct Svf {
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    #[inline]
    pub fn new() -> Self {
        Self {
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1eq: 0.0,
            ic2eq: 0.0,
        }
    }

    #[inline]
    pub fn lowpass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let mut s = Self::new();
        s.set_lowpass(sample_rate, cutoff_hz, q);
        s
    }

    #[inline]
    pub fn set_lowpass(&mut self, sample_rate: f32, cutoff_hz: f32, q: f32) {
        let sr = sample_rate.max(1.0);
        let cutoff = cutoff_hz.clamp(10.0, 0.45 * sr);
        let res = q.max(0.05);
        let g = (core::f32::consts::PI * (cutoff / sr)).tan();
        let k = 1.0 / res;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        self.g = g;
        self.k = k;
        self.a1 = a1;
        self.a2 = a2;
        self.a3 = a3;
    }

    #[inline]
    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        v2
    }
}
