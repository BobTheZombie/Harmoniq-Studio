#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(feature = "fast-math", allow(clippy::excessive_precision))]

pub mod biquad;
pub mod buffer;
pub mod delay;
pub mod gain;
pub mod pan;
pub mod saturator;
pub mod smoothing;
pub mod utils;

pub use buffer::{AudioBlock, AudioBlockMut, ChanMut, ChanRef};

#[cfg(feature = "simd")]
pub mod simd;
