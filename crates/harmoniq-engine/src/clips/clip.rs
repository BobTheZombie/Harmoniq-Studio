use std::sync::Arc;

use super::{ClipError, FadeCurve, FadeSpec};

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

#[derive(Debug, Clone)]
pub struct AudioClip {
    inner: Arc<ClipData>,
}

#[derive(Debug, Clone)]
struct ClipData {
    sample_rate: f32,
    channels: Vec<Vec<f32>>,
}

impl AudioClip {
    pub fn from_channels(channels: Vec<Vec<f32>>) -> Self {
        Self::with_sample_rate(DEFAULT_SAMPLE_RATE, channels)
    }

    pub fn with_sample_rate(sample_rate: f32, channels: Vec<Vec<f32>>) -> Self {
        let frames = validate_channels(&channels);
        let data = ClipData {
            sample_rate: sample_rate.max(0.0),
            channels: channels
                .into_iter()
                .map(|channel| {
                    let mut channel = channel;
                    channel.resize(frames, 0.0);
                    channel
                })
                .collect(),
        };
        Self {
            inner: Arc::new(data),
        }
    }

    pub fn empty(sample_rate: f32, channels: usize) -> Self {
        Self::with_sample_rate(sample_rate, vec![Vec::new(); channels])
    }

    pub fn sample_rate(&self) -> f32 {
        self.inner.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.inner.channels.len()
    }

    pub fn frames(&self) -> usize {
        validate_channels(&self.inner.channels)
    }

    pub fn samples(&self) -> &[Vec<f32>] {
        &self.inner.channels
    }

    pub fn channel(&self, index: usize) -> Option<&[f32]> {
        self.inner
            .channels
            .get(index)
            .map(|channel| channel.as_slice())
    }

    pub fn cloned_channels(&self) -> Vec<Vec<f32>> {
        self.inner.channels.clone()
    }

    pub fn with_gain(&self, gain: f32) -> Self {
        self.map_channels(|channel| {
            for sample in channel {
                *sample *= gain;
            }
        })
    }

    pub fn with_fade_in(&self, spec: FadeSpec) -> Result<Self, ClipError> {
        spec.validate(self.frames())?;
        Ok(self.map_channels(|channel| spec.apply_in(channel)))
    }

    pub fn with_fade_out(&self, spec: FadeSpec) -> Result<Self, ClipError> {
        spec.validate(self.frames())?;
        Ok(self.map_channels(|channel| spec.apply_out(channel)))
    }

    pub fn crossfade_with(
        &self,
        other: &AudioClip,
        overlap: usize,
        curve: FadeCurve,
    ) -> Result<Self, ClipError> {
        super::crossfade::crossfade(
            self,
            other,
            super::crossfade::CrossfadeSpec { overlap, curve },
        )
    }

    pub fn time_stretch(
        &self,
        ratio: f32,
        quality: super::stretch::StretchQuality,
    ) -> Result<Self, ClipError> {
        super::stretch::stretch_clip(self, ratio, quality)
    }

    fn map_channels<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&mut Vec<f32>),
    {
        let mut channels = self.inner.channels.clone();
        for channel in &mut channels {
            f(channel);
        }
        Self::with_sample_rate(self.sample_rate(), channels)
    }
}

fn validate_channels(channels: &[Vec<f32>]) -> usize {
    channels
        .iter()
        .map(|channel| channel.len())
        .max()
        .unwrap_or(0)
}
