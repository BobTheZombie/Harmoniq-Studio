//! Built-in Harmoniq plugins providing essential processing blocks.

pub mod dynamics;
pub mod generators;

pub use dynamics::GainPlugin;
pub use generators::{NoisePlugin, SineSynth};
