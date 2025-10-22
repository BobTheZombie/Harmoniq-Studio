//! Built-in Harmoniq plugins providing essential processing blocks.

use harmoniq_plugin_sdk::PluginModule;

pub mod dynamics;
pub mod effects;
pub mod generators;
pub mod instruments;

pub use dynamics::{GainPlugin, GainPluginFactory};
pub use effects::{
    AutoFilterFactory, AutoFilterPlugin, ChorusFactory, ChorusPlugin, CompressorFactory,
    CompressorPlugin, DelayFactory, DelayPlugin, DistortionFactory, DistortionPlugin,
    FlangerFactory, FlangerPlugin, LimiterFactory, LimiterPlugin, NoiseGateFactory,
    NoiseGatePlugin, ParametricEqFactory, ParametricEqPlugin, PhaserFactory, PhaserPlugin,
    ReverbFactory, ReverbPlugin, StereoEnhancerFactory, StereoEnhancerPlugin,
};
pub use generators::{NoisePlugin, NoisePluginFactory, SineSynth, SineSynthFactory};
pub use instruments::{
    AdditiveSynth, AdditiveSynthFactory, AnalogSynth, AnalogSynthFactory, BassSynth,
    BassSynthFactory, FmSynth, FmSynthFactory, GranularSynth, GranularSynthFactory,
    OrganPianoEngine, OrganPianoFactory, Sampler, SamplerFactory, Sub808, Sub808Factory,
    WavetableSynth, WavetableSynthFactory, WestCoastLead, WestCoastLeadFactory,
};

/// Returns a [`PluginModule`] containing all built-in Harmoniq processors.
pub fn builtin_module() -> PluginModule {
    let mut module = PluginModule::new();
    module
        .register_factory(Box::new(GainPluginFactory))
        .register_factory(Box::new(SineSynthFactory))
        .register_factory(Box::new(NoisePluginFactory))
        .register_factory(Box::new(AnalogSynthFactory))
        .register_factory(Box::new(FmSynthFactory))
        .register_factory(Box::new(WavetableSynthFactory))
        .register_factory(Box::new(SamplerFactory))
        .register_factory(Box::new(GranularSynthFactory))
        .register_factory(Box::new(AdditiveSynthFactory))
        .register_factory(Box::new(OrganPianoFactory))
        .register_factory(Box::new(BassSynthFactory))
        .register_factory(Box::new(Sub808Factory))
        .register_factory(Box::new(WestCoastLeadFactory))
        .register_factory(Box::new(ParametricEqFactory))
        .register_factory(Box::new(CompressorFactory))
        .register_factory(Box::new(LimiterFactory))
        .register_factory(Box::new(ReverbFactory))
        .register_factory(Box::new(DelayFactory))
        .register_factory(Box::new(ChorusFactory))
        .register_factory(Box::new(FlangerFactory))
        .register_factory(Box::new(PhaserFactory))
        .register_factory(Box::new(DistortionFactory))
        .register_factory(Box::new(AutoFilterFactory))
        .register_factory(Box::new(StereoEnhancerFactory))
        .register_factory(Box::new(NoiseGateFactory));
    module
}
