//! External plugin hosting layers for Harmoniq Studio.
//!
//! This crate provides thin wrappers around the dynamic entry points exposed by
//! common third-party plugin formats. The wrappers validate binary entry points
//! and keep the underlying dynamic library alive for the lifetime of the hosted
//! plugin instance. High-level integration with the Harmoniq engine can build on
//! top of these primitives to translate format-specific APIs into the
//! `AudioProcessor` trait expected by the engine.

mod error;
pub mod formats;

use std::path::Path;

pub use error::HostError;
pub use formats::{
    audio_unit::AudioUnitHost, linux_vst::LinuxVstHost, rtas::RtasHost, vst2::Vst2Host,
    vst3::Vst3Host,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Third-party plugin binary formats supported by the Harmoniq host layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PluginBinaryFormat {
    /// Legacy LinuxVST binaries (VST2 ABI with Linux-specific conventions).
    LinuxVst,
    /// Steinberg VST2 binaries distributed for desktop platforms.
    Vst2,
    /// Steinberg VST3 binaries with COM-like factories.
    Vst3,
    /// Apple AudioUnit components.
    AudioUnit,
    /// Digidesign Real-Time AudioSuite binaries.
    Rtas,
}

/// Ergonomic wrapper around the format-specific host implementations.
#[derive(Debug)]
pub enum HostLayer {
    LinuxVst(LinuxVstHost),
    Vst2(Vst2Host),
    Vst3(Vst3Host),
    AudioUnit(AudioUnitHost),
    Rtas(RtasHost),
}

impl HostLayer {
    /// Load a third-party plugin binary using the appropriate host layer.
    pub fn load(format: PluginBinaryFormat, path: impl AsRef<Path>) -> Result<Self, HostError> {
        match format {
            PluginBinaryFormat::LinuxVst => LinuxVstHost::load(path).map(HostLayer::LinuxVst),
            PluginBinaryFormat::Vst2 => Vst2Host::load(path).map(HostLayer::Vst2),
            PluginBinaryFormat::Vst3 => Vst3Host::load(path).map(HostLayer::Vst3),
            PluginBinaryFormat::AudioUnit => AudioUnitHost::load(path).map(HostLayer::AudioUnit),
            PluginBinaryFormat::Rtas => RtasHost::load(path).map(HostLayer::Rtas),
        }
    }

    /// Return the binary format handled by this host layer.
    pub fn format(&self) -> PluginBinaryFormat {
        match self {
            HostLayer::LinuxVst(_) => PluginBinaryFormat::LinuxVst,
            HostLayer::Vst2(_) => PluginBinaryFormat::Vst2,
            HostLayer::Vst3(_) => PluginBinaryFormat::Vst3,
            HostLayer::AudioUnit(_) => PluginBinaryFormat::AudioUnit,
            HostLayer::Rtas(_) => PluginBinaryFormat::Rtas,
        }
    }
}
