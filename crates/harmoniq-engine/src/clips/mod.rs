mod clip;
mod crossfade;
mod fade;
mod stretch;

pub use clip::AudioClip;
pub use crossfade::crossfade;
pub use fade::{FadeCurve, FadeSpec};
pub use stretch::StretchQuality;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipError {
    #[error("clip sample rate mismatch: {expected} vs {actual}")]
    SampleRateMismatch { expected: f32, actual: f32 },
    #[error("clip channel mismatch: {expected} vs {actual}")]
    ChannelMismatch { expected: usize, actual: usize },
    #[error("fade length {length} exceeds clip length {frames}")]
    FadeOutOfRange { length: usize, frames: usize },
    #[error("crossfade overlap must be greater than zero")]
    InvalidOverlap,
    #[error("stretch ratio must be positive")]
    InvalidStretchRatio,
}

const SAMPLE_RATE_EPSILON: f32 = 1e-3;

fn ensure_sample_rate_match(a: f32, b: f32) -> Result<(), ClipError> {
    if (a - b).abs() > SAMPLE_RATE_EPSILON {
        return Err(ClipError::SampleRateMismatch {
            expected: a,
            actual: b,
        });
    }
    Ok(())
}

fn ensure_channel_match(a: usize, b: usize) -> Result<(), ClipError> {
    if a != b {
        return Err(ClipError::ChannelMismatch {
            expected: a,
            actual: b,
        });
    }
    Ok(())
}

pub use crossfade::CrossfadeSpec;
