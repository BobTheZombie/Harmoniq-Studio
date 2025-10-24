//! Simplified sandboxed plugin bridge used for tests and offline development.

pub mod audio_ring;
pub mod ipc;
pub mod server;

pub use audio_ring::*;
pub use ipc::*;
pub use server::*;
