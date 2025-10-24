//! Built-in reference nodes for quick experimentation and the default graph.

pub mod gain;
pub mod noise;
pub mod sine;

pub use gain::GainNode;
pub use noise::{NodeNoise, NoiseNode};
pub use sine::{NodeOsc, SineNode};
