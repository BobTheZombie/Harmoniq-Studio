#[derive(Clone, Copy, Debug)]
pub struct LadderFilter {
    sample_rate: f32,
    cutoff: f32,
    resonance: f32,
    g: f32,
    stage: [f32; 4],
}

impl LadderFilter {
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = Self {
            sample_rate: sample_rate.max(1.0),
            cutoff: 1000.0,
            resonance: 0.0,
            g: 0.0,
            stage: [0.0; 4],
        };
        filter.update_coefficients();
        filter
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.update_coefficients();
    }

    pub fn set_params(&mut self, cutoff: f32, resonance: f32) {
        self.cutoff = cutoff;
        self.resonance = resonance.clamp(0.0, 0.95);
        self.update_coefficients();
    }

    #[inline]
    fn update_coefficients(&mut self) {
        let fc = self.cutoff.clamp(20.0, 20_000.0);
        let x = (std::f32::consts::PI * fc / self.sample_rate).min(1.0);
        // Bilinear transform approximation
        self.g = (x).tan();
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let k = 4.0 * self.resonance;
        let g = self.g;
        let g1 = g / (1.0 + g);
        let feedback = self.stage[3];
        let mut x = (input - k * feedback).tanh();

        for stage in &mut self.stage {
            let v = (x - *stage) * g1;
            x = v + *stage;
            *stage = x + v;
        }

        self.stage[3]
    }

    pub fn reset(&mut self) {
        self.stage = [0.0; 4];
    }
}
