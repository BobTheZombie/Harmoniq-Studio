use harmoniq_dsp::gain::{db_to_linear, linear_to_db};
use harmoniq_dsp::smoothing::OnePole;

use crate::buffer::AudioBuffer;

#[derive(Clone, Debug)]
pub struct FaderNode {
    target_db: f32,
    mute: bool,
    invert_phase: bool,
    smoother: OnePole,
    sample_rate: f32,
}

impl FaderNode {
    pub fn new(initial_db: f32) -> Self {
        Self {
            target_db: initial_db,
            mute: false,
            invert_phase: false,
            smoother: OnePole::new(48_000.0, 2.5),
            sample_rate: 48_000.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.smoother.set_time_ms(self.sample_rate, 2.5);
        let initial = if self.mute {
            0.0
        } else {
            db_to_linear(self.target_db)
        };
        self.smoother.reset(initial);
    }

    pub fn set_db(&mut self, value: f32) {
        self.target_db = value;
    }

    pub fn db(&self) -> f32 {
        self.target_db
    }

    pub fn set_mute(&mut self, mute: bool) {
        self.mute = mute;
    }

    pub fn mute(&self) -> bool {
        self.mute
    }

    pub fn set_phase_invert(&mut self, invert: bool) {
        self.invert_phase = invert;
    }

    pub fn phase_invert(&self) -> bool {
        self.invert_phase
    }

    pub fn current_gain_db(&self) -> f32 {
        linear_to_db(self.smoother.state().max(1e-6))
    }

    pub fn process_buffer(&mut self, buffer: &mut AudioBuffer) {
        if buffer.is_empty() {
            return;
        }
        let invert = if self.invert_phase { -1.0 } else { 1.0 };
        let target = if self.mute {
            0.0
        } else {
            db_to_linear(self.target_db)
        };
        let frames = buffer.len();
        let channels = buffer.channel_count();
        let data = buffer.as_mut_slice();
        for frame in 0..frames {
            let gain = self.smoother.next(target) * invert;
            for ch in 0..channels {
                let index = ch * frames + frame;
                data[index] *= gain;
            }
        }
    }
}

impl Default for FaderNode {
    fn default() -> Self {
        Self::new(0.0)
    }
}
