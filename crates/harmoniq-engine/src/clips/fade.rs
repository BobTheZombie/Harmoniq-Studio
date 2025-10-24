use std::f32::consts::FRAC_PI_2;

use super::ClipError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FadeCurve {
    Linear,
    EqualPower,
}

#[derive(Debug, Clone, Copy)]
pub struct FadeSpec {
    length: usize,
    curve: FadeCurve,
}

impl FadeSpec {
    pub fn new(length: usize, curve: FadeCurve) -> Self {
        Self { length, curve }
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn curve(&self) -> FadeCurve {
        self.curve
    }

    pub fn validate(&self, frames: usize) -> Result<(), ClipError> {
        if self.length > frames {
            return Err(ClipError::FadeOutOfRange {
                length: self.length,
                frames,
            });
        }
        Ok(())
    }

    pub(crate) fn apply_in(&self, channel: &mut [f32]) {
        let len = self.length.min(channel.len());
        if len == 0 {
            return;
        }
        for i in 0..len {
            let gain = self.gain_in_at(i);
            channel[i] *= gain;
        }
    }

    pub(crate) fn apply_out(&self, channel: &mut [f32]) {
        let len = self.length.min(channel.len());
        if len == 0 {
            return;
        }
        let start = channel.len().saturating_sub(len);
        for (idx, sample) in channel[start..].iter_mut().enumerate() {
            let gain = self.gain_out_at(idx);
            *sample *= gain;
        }
    }

    pub(crate) fn gain_in_at(&self, index: usize) -> f32 {
        if self.length <= 1 {
            return 1.0;
        }
        let progress =
            (index as f32).clamp(0.0, (self.length - 1) as f32) / (self.length - 1) as f32;
        self.curve.gain_in(progress)
    }

    pub(crate) fn gain_out_at(&self, index: usize) -> f32 {
        if self.length <= 1 {
            return 0.0;
        }
        let progress =
            (index as f32).clamp(0.0, (self.length - 1) as f32) / (self.length - 1) as f32;
        self.curve.gain_out(progress)
    }
}

impl FadeCurve {
    pub(crate) fn gain_in(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            FadeCurve::Linear => progress,
            FadeCurve::EqualPower => (FRAC_PI_2 * progress).sin(),
        }
    }

    pub(crate) fn gain_out(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            FadeCurve::Linear => 1.0 - progress,
            FadeCurve::EqualPower => (FRAC_PI_2 * progress).cos(),
        }
    }
}
