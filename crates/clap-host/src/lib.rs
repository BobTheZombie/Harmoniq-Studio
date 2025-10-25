//! Safe(-ish) wrappers around the CLAP ABI for hosting plug-ins.

mod discover;
mod events;
mod gui;
mod instance;
mod params;

pub use discover::{ClapLibrary, ClapPluginDescriptor, PluginDiscovery};
pub use events::{ClapEventQueue, EventSlice, EventWriter};
pub use gui::{GuiAttachRequest, GuiHandle};
pub use instance::{ActivationError, AudioConfig, ClapInstance};
pub use params::{ParamValue, ParameterQuery};

/// Re-export the raw bindings for users that need to drop down to the ABI.
pub use clap_sys as ffi;
