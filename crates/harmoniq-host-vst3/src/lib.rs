//! Harmoniq Studio VST3 hosting support.
//!
//! This crate mirrors the CLAP hosting infrastructure but targets Steinberg's
//! VST3 plugins.  It exposes a brokered hosting model that keeps third party
//! binaries in a sandboxed helper process while the main application communicates
//! over the same IPC primitives that power the CLAP host.  The host keeps track
//! of preset/state data, plugin delay compensation, and editor window handles so
//! callers can provide a unified experience regardless of the backing adapter
//! (official SDK or the OpenVST3 shim).

pub mod adapter;
pub mod broker;
pub mod host;
pub mod ipc;
pub mod pdc;
pub mod ring;
pub mod window;

pub use adapter::{AdapterDescriptor, AdapterKind, SandboxRequest};
pub use broker::{BrokerConfig, PluginBroker};
pub use host::{HostOptions, Vst3Host, Vst3HostBuilder};
pub use ipc::{BrokerCommand, BrokerEvent, RtChannel, RtMessage, RtMessageKind};
pub use pdc::{PdcEvent, PluginDataCache};
pub use ring::{SharedAudioRing, SharedAudioRingDescriptor};
pub use window::{WaylandEmbedder, WindowEmbedder, X11Embedder};
