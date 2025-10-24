use super::ClipError;
use crate::clips::AudioClip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StretchQuality {
    RealtimePreview,
    OfflineHighQuality,
}

pub fn stretch_clip(
    clip: &AudioClip,
    ratio: f32,
    quality: StretchQuality,
) -> Result<AudioClip, ClipError> {
    if ratio <= 0.0 {
        return Err(ClipError::InvalidStretchRatio);
    }
    if clip.frames() == 0 {
        return Ok(clip.clone());
    }

    let mut channels = Vec::with_capacity(clip.channels());
    for index in 0..clip.channels() {
        let source = clip.channel(index).unwrap_or(&[]);
        let stretched = match quality {
            StretchQuality::RealtimePreview => linear_resample(source, ratio),
            StretchQuality::OfflineHighQuality => cubic_resample(source, ratio),
        };
        channels.push(stretched);
    }

    Ok(AudioClip::with_sample_rate(clip.sample_rate(), channels))
}

fn linear_resample(source: &[f32], ratio: f32) -> Vec<f32> {
    resample_channel(source, ratio, Interpolation::Linear)
}

fn cubic_resample(source: &[f32], ratio: f32) -> Vec<f32> {
    resample_channel(source, ratio, Interpolation::Cubic)
}

enum Interpolation {
    Linear,
    Cubic,
}

fn resample_channel(source: &[f32], ratio: f32, interpolation: Interpolation) -> Vec<f32> {
    if source.is_empty() {
        return Vec::new();
    }

    let target_frames = ((source.len() as f32) * ratio).max(1.0).round() as usize;
    let mut output = Vec::with_capacity(target_frames);
    let step = 1.0 / ratio;

    for frame in 0..target_frames {
        let position = frame as f32 * step;
        let base = position.floor();
        let fraction = position - base;
        let base_index = base as isize;
        let sample = match interpolation {
            Interpolation::Linear => {
                let a = sample_at(source, base_index);
                let b = sample_at(source, base_index + 1);
                a + (b - a) * fraction
            }
            Interpolation::Cubic => {
                let p0 = sample_at(source, base_index - 1);
                let p1 = sample_at(source, base_index);
                let p2 = sample_at(source, base_index + 1);
                let p3 = sample_at(source, base_index + 2);
                catmull_rom(p0, p1, p2, p3, fraction)
            }
        };
        output.push(sample);
    }

    output
}

fn sample_at(source: &[f32], index: isize) -> f32 {
    if index <= 0 {
        return *source.first().unwrap_or(&0.0);
    }
    let index = index as usize;
    if index >= source.len() {
        *source.last().unwrap_or(&0.0)
    } else {
        source[index]
    }
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}
