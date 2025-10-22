use serde::{Deserialize, Serialize};

pub type ParameterId = u32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ParameterValue {
    pub id: ParameterId,
    pub value: f32,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ParameterSet {
    values: Vec<ParameterValue>,
}

impl ParameterSet {
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    pub fn set(&mut self, id: ParameterId, value: f32) {
        if let Some(entry) = self.values.iter_mut().find(|entry| entry.id == id) {
            entry.value = value;
        } else {
            self.values.push(ParameterValue { id, value });
        }
    }

    pub fn value(&self, id: ParameterId) -> Option<f32> {
        self.values
            .iter()
            .find(|entry| entry.id == id)
            .map(|v| v.value)
    }
}

pub struct ParameterView<'a> {
    set: &'a ParameterSet,
}

impl<'a> ParameterView<'a> {
    pub fn new(set: &'a ParameterSet) -> Self {
        Self { set }
    }

    pub fn value(&self, id: ParameterId, default: f32) -> f32 {
        self.set.value(id).unwrap_or(default)
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct AutomationLane {
    pub parameter: ParameterId,
    pub points: Vec<AutomationPoint>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub position_samples: u64,
    pub value: f32,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct AutomationData {
    lanes: Vec<AutomationLane>,
}

impl AutomationData {
    pub fn new() -> Self {
        Self { lanes: Vec::new() }
    }

    pub fn add_lane(&mut self, lane: AutomationLane) {
        self.lanes.push(lane);
    }

    pub fn lanes(&self) -> &[AutomationLane] {
        &self.lanes
    }
}
