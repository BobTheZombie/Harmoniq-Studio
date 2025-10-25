use std::f32::consts::TAU;

#[inline]
fn wrap_phase(phase: f32) -> f32 {
    if phase >= TAU {
        phase - TAU
    } else if phase < 0.0 {
        phase + TAU
    } else {
        phase
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Oscillator {
    phase: f32,
}

impl Oscillator {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    #[inline]
    pub fn advance_sine(&mut self, freq: f32, sample_rate: f32) -> f32 {
        let incr = (freq / sample_rate).clamp(0.0, 0.5) * TAU;
        self.phase = wrap_phase(self.phase + incr);
        self.phase.sin()
    }

    #[inline]
    pub fn advance_saw(&mut self, freq: f32, sample_rate: f32) -> f32 {
        let incr = (freq / sample_rate).clamp(0.0, 0.5) * TAU;
        self.phase = wrap_phase(self.phase + incr);
        // Map [0, TAU) to [-1, 1]
        (self.phase / std::f32::consts::PI) - 1.0
    }

    #[inline]
    pub fn advance_triangle(&mut self, freq: f32, sample_rate: f32) -> f32 {
        let incr = (freq / sample_rate).clamp(0.0, 0.5) * TAU;
        self.phase = wrap_phase(self.phase + incr);
        // Triangle from saw
        let saw = (self.phase / std::f32::consts::PI) - 1.0;
        (saw.abs() * 2.0 - 1.0).clamp(-1.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LfoWaveform {
    Sine,
    Triangle,
}

#[derive(Clone, Copy, Debug)]
pub struct Lfo {
    phase: f32,
}

impl Lfo {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    #[inline]
    pub fn next(&mut self, waveform: LfoWaveform, rate_hz: f32, sample_rate: f32) -> f32 {
        let incr = (rate_hz / sample_rate).max(0.0) * TAU;
        self.phase = wrap_phase(self.phase + incr);
        match waveform {
            LfoWaveform::Sine => self.phase.sin(),
            LfoWaveform::Triangle => {
                let saw = (self.phase / std::f32::consts::PI) - 1.0;
                (saw.abs() * 2.0 - 1.0).clamp(-1.0, 1.0)
            }
        }
    }
}
