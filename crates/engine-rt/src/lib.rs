//! Real-time primitives shared across the Harmoniq audio engine.

pub mod callback;
pub mod queue;
pub mod transport;

pub use callback::{AudioCallbackInfo, AudioProcessor, CallbackHandle, InterleavedAudioBuffer};
pub use queue::{EventQueue, QueueError};
pub use transport::{TempoEvent, TempoMap, TransportCommand, TransportState};
