//! Lightweight profiling and timing utilities.

use std::time::{Duration, Instant};

/// Records a span of time for diagnostic purposes.
#[derive(Debug)]
pub struct SpanTimer {
    label: &'static str,
    start: Instant,
}

impl SpanTimer {
    /// Starts a new span timer.
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            start: Instant::now(),
        }
    }

    /// Finishes the span and logs its duration using [`tracing`].
    pub fn finish(self) -> Duration {
        let duration = self.start.elapsed();
        tracing::trace!(target: "profiling", label = self.label, elapsed = ?duration);
        duration
    }
}

impl Drop for SpanTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        tracing::trace!(target: "profiling", label = self.label, elapsed = ?duration, "profiling span completed");
    }
}
