//! Ultra low latency sound server for Harmoniq.
//!
//! The sound server runs the [`HarmoniqEngine`] in a dedicated real-time
//! thread and streams audio directly to the selected ALSA device. The design
//! avoids blocking operations on the audio thread and keeps all allocations out
//! of the critical path to guarantee deterministic scheduling behaviour. This
//! module is only available on Linux builds where ALSA is present.

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::{UltraLowLatencyOptions, UltraLowLatencyServer};

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Clone)]
pub struct UltraLowLatencyOptions;

#[cfg(not(target_os = "linux"))]
pub struct UltraLowLatencyServer;

#[cfg(not(target_os = "linux"))]
impl UltraLowLatencyServer {
    /// Stub constructor for non-Linux targets. The custom sound server is not
    /// implemented outside of Linux at the moment. Callers should fall back to
    /// the platform specific host backends.
    pub fn start(
        _engine: std::sync::Arc<parking_lot::Mutex<crate::HarmoniqEngine>>,
        _config: crate::BufferConfig,
        _options: UltraLowLatencyOptions,
    ) -> anyhow::Result<Self> {
        anyhow::bail!("the Harmoniq ultra low latency server is only supported on Linux");
    }
}
