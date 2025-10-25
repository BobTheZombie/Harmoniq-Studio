//! Helpers for exporting CLAP plug-ins with safe Rust wrappers.

mod author;
pub mod export;

pub use author::{
    ActivationContext, AudioProcessor, Gui, Latency, NotePorts, Plugin, PluginDescriptor,
    PluginFactory, State, Tail,
};
