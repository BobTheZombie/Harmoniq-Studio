//! Ultra low latency sound server for Harmoniq.
//!
//! The sound server runs the [`HarmoniqEngine`] in a dedicated real-time
//! thread and streams audio directly to the selected ALSA or OpenASIO driver.
//! The design avoids blocking operations on the audio thread and keeps all
//! allocations out of the critical path to guarantee deterministic scheduling
//! behaviour. This module is only available on Linux builds where ALSA is
//! present.

#[cfg(all(target_os = "linux", feature = "native"))]
mod linux;

#[cfg(all(target_os = "linux", feature = "native", feature = "openasio"))]
pub use linux::UltraOpenAsioOptions;
#[cfg(all(target_os = "linux", feature = "native"))]
pub use linux::{UltraLowLatencyOptions, UltraLowLatencyServer};

#[cfg(all(target_os = "linux", feature = "native"))]
pub use linux::alsa_devices_available;

#[cfg(not(all(target_os = "linux", feature = "native")))]
#[derive(Debug, Clone)]
pub struct UltraLowLatencyOptions;

#[cfg(not(all(target_os = "linux", feature = "native")))]
pub struct UltraLowLatencyServer;

#[cfg(not(all(target_os = "linux", feature = "native")))]
impl UltraLowLatencyServer {
    /// Stub constructor for non-Linux targets. The custom sound server is not
    /// implemented outside of Linux at the moment. Callers should fall back to
    /// the platform specific host backends.
    pub fn start(
        _engine: std::sync::Arc<parking_lot::Mutex<crate::HarmoniqEngine>>,
        _config: crate::BufferConfig,
        _options: UltraLowLatencyOptions,
    ) -> anyhow::Result<Self> {
        anyhow::bail!(
            "the Harmoniq ultra low latency server is only supported on Linux with the native feature"
        );
    }
}
