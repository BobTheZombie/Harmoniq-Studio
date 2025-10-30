//! Constant-power pan law utilities for the mixer backend.
//!
//! Maps a pan position in the range [-1.0, 1.0] to linear gains for the left
//! and right channels using a -3 dB law at the center position.

#[inline(always)]
pub fn constant_power_pan_gains(pan: f32) -> (f32, f32) {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * core::f32::consts::FRAC_PI_2;
    (theta.cos(), theta.sin())
}
