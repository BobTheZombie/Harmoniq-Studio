use super::{ensure_channel_match, ensure_sample_rate_match, ClipError, FadeCurve, FadeSpec};
use crate::clips::AudioClip;

#[derive(Debug, Clone, Copy)]
pub struct CrossfadeSpec {
    pub overlap: usize,
    pub curve: FadeCurve,
}

impl CrossfadeSpec {
    pub fn new(overlap: usize, curve: FadeCurve) -> Self {
        Self { overlap, curve }
    }
}

pub fn crossfade(
    a: &AudioClip,
    b: &AudioClip,
    spec: CrossfadeSpec,
) -> Result<AudioClip, ClipError> {
    if spec.overlap == 0 {
        return Err(ClipError::InvalidOverlap);
    }
    ensure_sample_rate_match(a.sample_rate(), b.sample_rate())?;
    ensure_channel_match(a.channels(), b.channels())?;

    let overlap = spec.overlap.min(a.frames()).min(b.frames());

    if overlap == 0 {
        return Ok(append(a, b));
    }

    let total_frames = a.frames() + b.frames() - overlap;
    let mut channels = vec![vec![0.0f32; total_frames]; a.channels()];
    let fade = FadeSpec::new(overlap, spec.curve);
    let pre_length = a.frames().saturating_sub(overlap);

    for (channel_index, output_channel) in channels.iter_mut().enumerate() {
        let source_a = a.channel(channel_index).unwrap_or(&[]);
        let source_b = b.channel(channel_index).unwrap_or(&[]);

        if pre_length > 0 {
            output_channel[..pre_length].copy_from_slice(&source_a[..pre_length]);
        }

        for i in 0..overlap {
            let fade_in = fade.gain_in_at(i);
            let fade_out = fade.gain_out_at(i);
            let a_idx = pre_length + i;
            let b_idx = i;
            let a_sample = source_a.get(a_idx).copied().unwrap_or(0.0);
            let b_sample = source_b.get(b_idx).copied().unwrap_or(0.0);
            output_channel[pre_length + i] = a_sample * fade_out + b_sample * fade_in;
        }

        let tail = &mut output_channel[pre_length + overlap..];
        if !tail.is_empty() {
            tail.copy_from_slice(&source_b[overlap..]);
        }
    }

    Ok(AudioClip::with_sample_rate(a.sample_rate(), channels))
}

fn append(a: &AudioClip, b: &AudioClip) -> AudioClip {
    let total_frames = a.frames() + b.frames();
    let mut channels = vec![vec![0.0f32; total_frames]; a.channels()];
    for (channel_index, output_channel) in channels.iter_mut().enumerate() {
        let source_a = a.channel(channel_index).unwrap_or(&[]);
        let source_b = b.channel(channel_index).unwrap_or(&[]);
        let split = source_a.len();
        if split > 0 {
            output_channel[..split].copy_from_slice(source_a);
        }
        if !source_b.is_empty() {
            output_channel[split..split + source_b.len()].copy_from_slice(source_b);
        }
    }
    AudioClip::with_sample_rate(a.sample_rate(), channels)
}
