use engine_graph::automation::ParameterId;
use engine_graph::AudioNode;

pub const ENTRY_SYMBOL: &str = "harmoniq_plugin_entry";

#[derive(Clone, Debug)]
pub struct PluginDescriptor {
    pub name: &'static str,
    pub vendor: &'static str,
    pub version: &'static str,
}

#[derive(Clone, Debug)]
pub struct PluginParameterDescriptor {
    pub id: ParameterId,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

pub trait PluginInstance: Send {
    fn descriptor(&self) -> &PluginDescriptor;
    fn parameters(&self) -> &[PluginParameterDescriptor];
    fn node(&mut self) -> &mut dyn AudioNode;
}

pub trait PluginFactory: Send + Sync {
    fn descriptor(&self) -> &PluginDescriptor;
    fn create(&self) -> Box<dyn PluginInstance>;
}

#[allow(improper_ctypes_definitions)]
pub type PluginEntry = unsafe extern "C" fn() -> *mut dyn PluginFactory;

pub unsafe fn take_factory(entry: PluginEntry) -> Box<dyn PluginFactory> {
    Box::from_raw(entry())
}
