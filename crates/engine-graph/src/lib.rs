//! Audio graph infrastructure for the Harmoniq engine.

pub mod automation;
pub mod graph;
pub mod node;

pub use automation::{
    AutomationData, AutomationLane, AutomationPoint, ParameterId, ParameterSet, ParameterValue,
    ParameterView,
};
pub use graph::{AudioGraph, GraphBuilder, GraphConfig, GraphExecutor};
pub use node::{AudioNode, NodePreparation, PortBuffer, ProcessContext};

pub type NodeId = u64;
