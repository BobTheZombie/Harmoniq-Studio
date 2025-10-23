//! Shared utilities for the Harmoniq Studio workspace.

pub mod db;
pub mod profiling;
pub mod rt;
pub mod time;

pub use rt::{RtReceiver, RtSender};

/// Convenience type alias for sample positions measured in frames.
pub type SampleTime = u64;

/// Convenience type alias for values expressed in decibels.
pub type Decibels = f32;
