//! Harmoniq Studio CLAP hosting support.
//!
//! This crate provides utilities for scanning CLAP plugins, managing
//! brokered plugin processes, and exchanging real-time audio data via a
//! shared memory ring buffer.

pub mod broker;
pub mod cache;
pub mod host;
pub mod ipc;
pub mod pdc;
pub mod ring;
pub mod window;

pub use broker::{BrokerConfig, PluginBroker};
pub use cache::{PluginCacheEntry, PluginScanner};
pub use host::{ClapHost, HostOptions};
pub use ipc::{BrokerCommand, BrokerEvent, RtMessage, RtMessageKind};
pub use ring::{SharedAudioRing, SharedAudioRingDescriptor};
