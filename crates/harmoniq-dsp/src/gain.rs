use crate::buffer::{AudioBlock, AudioBlockMut};

#[derive(Clone, Copy, Debug)]
pub struct Gain {
    linear: f32,
}

impl Gain {
    #[inline]
    pub fn unity() -> Self {
        Self { linear: 1.0 }
    }

    #[inline]
    pub fn from_db(db: f32) -> Self {
        Self {
            linear: db_to_linear(db),
        }
    }

    #[inline]
    pub fn set_linear(&mut self, linear: f32) {
        self.linear = linear;
    }

    #[inline]
    pub fn linear(&self) -> f32 {
        self.linear
    }

    #[inline]
    pub fn process(&self, input: &AudioBlock<'_>, output: &mut AudioBlockMut<'_>) {
        let channels = output.channels().min(input.channels()) as usize;
        let frames = output.frames().min(input.frames()) as usize;
        for frame in 0..frames {
            for ch in 0..channels {
                let sample = unsafe { input.read_sample(ch, frame) };
                unsafe { output.write_sample(ch, frame, sample * self.linear) };
            }
        }
    }
}

#[inline]
pub fn db_to_linear(db: f32) -> f32 {
    if db <= -120.0 {
        0.0
    } else {
        10.0f32.powf(db * 0.05)
    }
}
