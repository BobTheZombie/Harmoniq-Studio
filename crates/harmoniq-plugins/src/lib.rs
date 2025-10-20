//! Built-in Harmoniq plugins providing essential processing blocks.

use harmoniq_plugin_sdk::PluginModule;

pub mod dynamics;
pub mod generators;

pub use dynamics::{GainPlugin, GainPluginFactory};
pub use generators::{NoisePlugin, NoisePluginFactory, SineSynth, SineSynthFactory};

/// Returns a [`PluginModule`] containing all built-in Harmoniq processors.
pub fn builtin_module() -> PluginModule {
    let mut module = PluginModule::new();
    module
        .register_factory(Box::new(GainPluginFactory))
        .register_factory(Box::new(SineSynthFactory))
        .register_factory(Box::new(NoisePluginFactory));
    module
}
