//! Harmoniq Plugin SDK
//! ====================
//!
//! Convenience utilities and abstractions for building Harmoniq-native
//! instruments and effects. The SDK builds on top of [`harmoniq_engine`]'s
//! [`AudioProcessor`](harmoniq_engine::AudioProcessor) trait and provides
//! helpers for describing plugin metadata, parameters, and module
//! registration.

mod parameters;
mod registry;

pub use parameters::{
    ContinuousParameterOptions, ParameterDefinition, ParameterId, ParameterKind, ParameterLayout,
    ParameterSet, ParameterValue, PluginParameterError,
};
pub use registry::{NativePlugin, PluginExport, PluginFactory, PluginModule};

/// Common imports for plugin authors implementing Harmoniq-native processors.
pub mod prelude {
    pub use crate::{
        ContinuousParameterOptions, NativePlugin, ParameterDefinition, ParameterId, ParameterKind,
        ParameterLayout, ParameterSet, ParameterValue, PluginExport, PluginFactory, PluginModule,
    };
    pub use harmoniq_engine::{
        AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, MidiEvent, MidiProcessor,
        PluginDescriptor,
    };
}

/// Declare the plugin entry point for a dynamic Harmoniq plugin module.
///
/// The macro expects one or more expressions that evaluate to types
/// implementing [`PluginFactory`]. Each factory will be registered within the
/// exported [`PluginModule`].
///
/// # Example
///
/// ```ignore
/// use harmoniq_plugin_sdk::{declare_harmoniq_plugins, PluginFactory, PluginModule};
///
/// struct MyFactory;
///
/// impl PluginFactory for MyFactory { /* ... */ }
///
/// declare_harmoniq_plugins!(MyFactory);
/// ```
#[macro_export]
macro_rules! declare_harmoniq_plugins {
    ($($factory:expr),+ $(,)?) => {
        #[no_mangle]
        pub extern "C" fn harmoniq_plugin_entrypoint() -> $crate::PluginExport {
            let mut module = $crate::PluginModule::new();
            $(module.register_factory(Box::new($factory));)+
            $crate::PluginExport::new(module)
        }
    };
}
