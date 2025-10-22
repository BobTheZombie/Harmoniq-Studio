pub mod filter;
pub mod oscillator;

pub use filter::{OnePoleLowPass, PARAM_CUTOFF};
pub use oscillator::{SineOscillator, PARAM_AMPLITUDE, PARAM_FREQUENCY};
