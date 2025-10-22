//! Lock-free audio callback wrappers shared between the engine and IO backends.

use std::cell::UnsafeCell;
use std::sync::Arc;

/// Audio callback metadata supplied by the backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioCallbackInfo {
    /// Presentation timestamp in nanoseconds if available.
    pub timestamp_ns: Option<u64>,
    /// Host provided capture latency in samples.
    pub input_latency: Option<u32>,
    /// Host provided playback latency in samples.
    pub output_latency: Option<u32>,
}

/// Interleaved audio buffer passed to the real-time processor.
pub struct InterleavedAudioBuffer<'a> {
    pub inputs: &'a [f32],
    pub outputs: &'a mut [f32],
    pub channels: usize,
    pub frames: usize,
    pub sample_rate: u32,
    pub info: AudioCallbackInfo,
}

impl<'a> InterleavedAudioBuffer<'a> {
    pub fn silence(&mut self) {
        self.outputs.fill(0.0);
    }
}

/// Trait implemented by the engine real-time renderer.
pub trait AudioProcessor: Send {
    fn process(&mut self, buffer: &mut InterleavedAudioBuffer<'_>);
}

struct CallbackCell {
    processor: UnsafeCell<Box<dyn AudioProcessor>>,
}

unsafe impl Send for CallbackCell {}
unsafe impl Sync for CallbackCell {}

/// A handle shared with IO backends for triggering real-time processing.
#[derive(Clone)]
pub struct CallbackHandle {
    inner: Arc<CallbackCell>,
}

impl CallbackHandle {
    pub fn new(processor: Box<dyn AudioProcessor>) -> Self {
        Self {
            inner: Arc::new(CallbackCell {
                processor: UnsafeCell::new(processor),
            }),
        }
    }

    pub fn process(&self, buffer: &mut InterleavedAudioBuffer<'_>) {
        // Safety: Audio backends call this from a single thread associated with the stream.
        unsafe {
            let processor = &mut *self.inner.processor.get();
            processor.process(buffer);
        }
    }
}
