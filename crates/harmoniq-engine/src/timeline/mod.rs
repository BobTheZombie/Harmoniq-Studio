use std::cmp::Ordering;

use thiserror::Error;

use crate::clips::{AudioClip, ClipError, FadeCurve, FadeSpec};

#[derive(Debug, Clone)]
pub struct ClipEvent {
    pub clip: AudioClip,
    pub start_frame: usize,
    pub gain: f32,
    pub fade_in: Option<FadeSpec>,
    pub fade_out: Option<FadeSpec>,
}

impl ClipEvent {
    pub fn new(clip: AudioClip, start_frame: usize) -> Self {
        Self {
            clip,
            start_frame,
            gain: 1.0,
            fade_in: None,
            fade_out: None,
        }
    }

    pub fn with_gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    pub fn with_fade_in(mut self, fade: FadeSpec) -> Self {
        self.fade_in = Some(fade);
        self
    }

    pub fn with_fade_out(mut self, fade: FadeSpec) -> Self {
        self.fade_out = Some(fade);
        self
    }
}

#[derive(Debug, Default)]
pub struct Timeline {
    sample_rate: f32,
    channels: usize,
    clips: Vec<ClipEvent>,
}

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error(transparent)]
    Clip(#[from] ClipError),
    #[error("clip fade exceeds clip length")]
    InvalidFade,
}

impl Timeline {
    pub fn new(sample_rate: f32, channels: usize) -> Self {
        Self {
            sample_rate,
            channels,
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: ClipEvent) {
        self.clips.push(clip);
    }

    pub fn clear(&mut self) {
        self.clips.clear();
    }

    pub fn render(&self) -> Result<AudioClip, TimelineError> {
        if self.channels == 0 {
            return Ok(AudioClip::empty(self.sample_rate, 0));
        }

        let mut events = self.clips.clone();
        events.sort_by(|a, b| match a.start_frame.cmp(&b.start_frame) {
            Ordering::Equal => a.clip.frames().cmp(&b.clip.frames()),
            other => other,
        });

        let total_frames = events
            .iter()
            .map(|event| event.start_frame + event.clip.frames())
            .max()
            .unwrap_or(0);

        let mut buffer = vec![vec![0.0f32; total_frames]; self.channels];

        for event in &events {
            self.render_event(event, &mut buffer)?;
        }

        Ok(AudioClip::with_sample_rate(self.sample_rate, buffer))
    }

    fn render_event(
        &self,
        event: &ClipEvent,
        buffer: &mut [Vec<f32>],
    ) -> Result<(), TimelineError> {
        let clip = &event.clip;
        if clip.frames() == 0 {
            return Ok(());
        }

        if let Some(fade) = event.fade_in {
            fade.validate(clip.frames())
                .map_err(|_| TimelineError::InvalidFade)?;
        }
        if let Some(fade) = event.fade_out {
            fade.validate(clip.frames())
                .map_err(|_| TimelineError::InvalidFade)?;
        }

        ensure_capacity(buffer, event.start_frame + clip.frames());

        for channel_index in 0..self.channels {
            let source = clip.channel(channel_index).unwrap_or_else(|| {
                clip.channel(clip.channels().saturating_sub(1))
                    .unwrap_or(&[])
            });
            let destination = &mut buffer[channel_index];
            mix_channel(
                destination,
                source,
                event.start_frame,
                event.gain,
                event.fade_in,
                event.fade_out,
            );
        }

        Ok(())
    }
}

fn ensure_capacity(buffer: &mut [Vec<f32>], frames: usize) {
    for channel in buffer {
        if channel.len() < frames {
            channel.resize(frames, 0.0);
        }
    }
}

fn mix_channel(
    destination: &mut [f32],
    source: &[f32],
    start: usize,
    gain: f32,
    fade_in: Option<FadeSpec>,
    fade_out: Option<FadeSpec>,
) {
    let fade_in = fade_in.unwrap_or(FadeSpec::new(0, FadeCurve::Linear));
    let fade_out = fade_out.unwrap_or(FadeSpec::new(0, FadeCurve::Linear));
    let fade_in_len = fade_in.length();
    let fade_out_len = fade_out.length();
    let frames = source.len();

    for (index, sample) in source.iter().enumerate() {
        let mut value = *sample * gain;
        if fade_in_len > 0 && index < fade_in_len {
            value *= fade_in.gain_in_at(index);
        }
        if fade_out_len > 0 && index >= frames.saturating_sub(fade_out_len) {
            let relative = index - (frames - fade_out_len);
            value *= fade_out.gain_out_at(relative);
        }
        let dest_index = start + index;
        if dest_index < destination.len() {
            destination[dest_index] += value;
        }
    }
}
