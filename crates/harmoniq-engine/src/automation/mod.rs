use crate::plugin::PluginId;

pub mod curve;
pub mod lane;
pub mod record;

pub use curve::{AutomationCurve, CurvePoint, CurveShape};
pub use lane::{AutomationCommand, AutomationLane, AutomationSender, ParameterSpec};
pub use record::{AutomationRecorder, AutomationWriteMode};

#[derive(Debug, Clone)]
pub struct AutomationEvent {
    pub plugin_id: PluginId,
    pub parameter: usize,
    pub value: f32,
    pub sample_offset: u32,
}
