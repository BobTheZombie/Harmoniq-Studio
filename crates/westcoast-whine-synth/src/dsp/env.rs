#[derive(Clone, Copy, Debug, PartialEq)]
enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Debug)]
pub struct AdsrEnvelope {
    sample_rate: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    level: f32,
    state: EnvelopeState,
    release_start: f32,
}

impl AdsrEnvelope {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            attack: 0.01,
            decay: 0.1,
            sustain: 0.8,
            release: 0.2,
            level: 0.0,
            state: EnvelopeState::Idle,
            release_start: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    pub fn set_params(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack = attack.max(1.0 / self.sample_rate);
        self.decay = decay.max(1.0 / self.sample_rate);
        self.sustain = sustain.clamp(0.0, 1.0);
        self.release = release.max(1.0 / self.sample_rate);
    }

    pub fn trigger(&mut self) {
        self.state = EnvelopeState::Attack;
        self.level = 0.0;
    }

    pub fn retrigger(&mut self) {
        self.state = EnvelopeState::Attack;
    }

    pub fn release(&mut self) {
        if self.state != EnvelopeState::Idle {
            self.release_start = self.level;
            self.state = EnvelopeState::Release;
        }
    }

    pub fn reset(&mut self) {
        self.state = EnvelopeState::Idle;
        self.level = 0.0;
        self.release_start = 0.0;
    }

    pub fn is_active(&self) -> bool {
        self.state != EnvelopeState::Idle
    }

    pub fn next_sample(&mut self) -> f32 {
        match self.state {
            EnvelopeState::Idle => {
                self.level = 0.0;
            }
            EnvelopeState::Attack => {
                let step = if self.attack <= 0.0 {
                    1.0
                } else {
                    1.0 / (self.attack * self.sample_rate)
                };
                self.level += step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.state = EnvelopeState::Decay;
                }
            }
            EnvelopeState::Decay => {
                let step = if self.decay <= 0.0 {
                    self.level
                } else {
                    (1.0 - self.sustain) / (self.decay * self.sample_rate)
                };
                self.level -= step;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.state = EnvelopeState::Sustain;
                }
            }
            EnvelopeState::Sustain => {
                self.level = self.sustain;
            }
            EnvelopeState::Release => {
                let step = if self.release <= 0.0 {
                    self.release_start
                } else {
                    self.release_start / (self.release * self.sample_rate)
                };
                self.level -= step;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.state = EnvelopeState::Idle;
                }
            }
        }

        self.level = self.level.clamp(0.0, 1.0);
        self.level
    }
}
