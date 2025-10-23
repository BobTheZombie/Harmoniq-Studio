//! Audio and MIDI routing graph primitives.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unique identifier for nodes stored inside the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

impl From<usize> for NodeId {
    fn from(value: usize) -> Self {
        NodeId(value as u32)
    }
}

impl From<NodeId> for usize {
    fn from(value: NodeId) -> Self {
        value.0 as usize
    }
}

/// Identifier for a specific pin on a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PinId {
    /// Node owning the pin.
    pub node: NodeId,
    /// Pin index within the node.
    pub pin: u8,
}

/// Collection of graph nodes and their routing.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Graph {
    /// Node storage with stable indices.
    pub nodes: Vec<Option<Node>>,
    /// List of connections between pins.
    pub edges: Vec<Edge>,
    /// Cached plugin delay compensation state.
    pub pdc: PdcState,
}

impl Graph {
    /// Creates an empty graph instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node to the graph.
    pub fn add_node(&mut self, kind: NodeKind, params: NodeParams) -> NodeId {
        if let Some((index, slot)) = self
            .nodes
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            let id = NodeId(index as u32);
            *slot = Some(Node {
                id,
                kind,
                params,
                latency_samples: 0,
            });
            return id;
        }
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Some(Node {
            id,
            kind,
            params,
            latency_samples: 0,
        }));
        id
    }

    /// Removes a node from the graph.
    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        let idx: usize = id.into();
        if let Some(slot) = self.nodes.get_mut(idx) {
            let removed = slot.take();
            if removed.is_some() {
                self.edges
                    .retain(|edge| edge.from.node != id && edge.to.node != id);
                self.recompute_pdc();
            }
            removed
        } else {
            None
        }
    }

    /// Adds a connection between two pins.
    pub fn connect(&mut self, from: PinId, to: PinId, gain: f32) -> Result<(), GraphError> {
        if from.node == to.node {
            return Err(GraphError::SelfConnection(from.node));
        }
        if self.node(from.node).is_none() || self.node(to.node).is_none() {
            return Err(GraphError::MissingNode);
        }
        if self
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to)
        {
            return Err(GraphError::DuplicateEdge);
        }
        self.edges.push(Edge { from, to, gain });
        Ok(())
    }

    /// Recomputes plugin delay compensation for the current topology.
    pub fn recompute_pdc(&mut self) {
        let mut latencies = vec![0u32; self.nodes.len()];
        let mut indegrees: HashMap<NodeId, usize> = HashMap::new();
        let mut adjacency: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        for edge in &self.edges {
            if self.node(edge.from.node).is_none() || self.node(edge.to.node).is_none() {
                continue;
            }
            *indegrees.entry(edge.to.node).or_default() += 1;
            adjacency
                .entry(edge.from.node)
                .or_default()
                .push(edge.to.node);
        }

        let mut queue = VecDeque::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let Some(node) = node else { continue };
            let id = NodeId(index as u32);
            if indegrees.get(&id).copied().unwrap_or(0) == 0 {
                queue.push_back(id);
            }
            latencies[index] = node.latency_samples;
        }

        while let Some(node_id) = queue.pop_front() {
            let node_latency = {
                let idx: usize = node_id.into();
                latencies[idx]
            };
            if let Some(targets) = adjacency.get(&node_id) {
                for &target in targets {
                    let target_idx: usize = target.into();
                    if self.node(target).is_none() {
                        continue;
                    }
                    let target_latency = self
                        .node(target)
                        .map(|node| node.latency_samples)
                        .unwrap_or(0);
                    let candidate = node_latency + target_latency;
                    latencies[target_idx] = latencies[target_idx].max(candidate);
                    if let Some(indegree) = indegrees.get_mut(&target) {
                        *indegree = indegree.saturating_sub(1);
                        if *indegree == 0 {
                            queue.push_back(target);
                        }
                    }
                }
            }
        }

        let max_latency = latencies.iter().copied().max().unwrap_or(0);
        self.pdc = PdcState {
            latency_per_node: latencies,
            max_latency,
        };
    }

    /// Retrieves a node by identifier.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        let idx: usize = id.into();
        self.nodes.get(idx).and_then(|slot| slot.as_ref())
    }

    /// Updates the latency of a node and refreshes PDC state.
    pub fn set_latency(&mut self, id: NodeId, latency_samples: u32) {
        let idx: usize = id.into();
        if let Some(slot) = self.nodes.get_mut(idx) {
            if let Some(node) = slot.as_mut() {
                node.latency_samples = latency_samples;
                self.recompute_pdc();
            }
        }
    }

    /// Provides a topologically sorted node list used for rendering.
    pub fn topological_order(&self) -> Vec<NodeId> {
        let mut indegrees: HashMap<NodeId, usize> = HashMap::new();
        let mut adjacency: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for edge in &self.edges {
            if self.node(edge.from.node).is_none() || self.node(edge.to.node).is_none() {
                continue;
            }
            *indegrees.entry(edge.to.node).or_default() += 1;
            adjacency
                .entry(edge.from.node)
                .or_default()
                .push(edge.to.node);
        }
        let mut order = Vec::new();
        let mut queue = VecDeque::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if node.is_some() {
                let id = NodeId(index as u32);
                if indegrees.get(&id).copied().unwrap_or(0) == 0 {
                    queue.push_back(id);
                }
            }
        }
        while let Some(node) = queue.pop_front() {
            order.push(node);
            if let Some(targets) = adjacency.get(&node) {
                for target in targets {
                    if let Some(indegree) = indegrees.get_mut(target) {
                        *indegree = indegree.saturating_sub(1);
                        if *indegree == 0 {
                            queue.push_back(*target);
                        }
                    }
                }
            }
        }
        order
    }
}

/// Error produced by graph manipulation operations.
#[derive(Debug, Error)]
pub enum GraphError {
    /// Attempted to connect a node to itself.
    #[error("self connection for node {0:?} is not allowed")]
    SelfConnection(NodeId),
    /// Attempted to reference an unknown node.
    #[error("node not found")]
    MissingNode,
    /// Duplicate edges are not supported.
    #[error("duplicate edge")]
    DuplicateEdge,
}

/// Node stored in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Identifier of the node within the graph.
    pub id: NodeId,
    /// Kind of processing performed by the node.
    pub kind: NodeKind,
    /// Node specific parameters.
    pub params: NodeParams,
    /// Latency contributed by the node in samples.
    pub latency_samples: u32,
}

/// Additional per-node parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeParams {
    /// Display name used in the UI.
    pub name: String,
    /// Trim applied to the incoming signal.
    pub gain: f32,
    /// Optional pan value (-1.0 = left, 1.0 = right).
    pub pan: f32,
}

impl Default for NodeParams {
    fn default() -> Self {
        Self {
            name: "Node".to_string(),
            gain: 1.0,
            pan: 0.0,
        }
    }
}

/// Types of nodes supported by the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    /// Audio track with audio inputs and outputs.
    AudioTrack,
    /// Instrument track generating audio from MIDI.
    InstrumentTrack,
    /// Return bus with an index.
    ReturnBus(u32),
    /// Master bus.
    MasterBus,
    /// Container for plugin inserts.
    PluginContainer(PluginChain),
    /// Audio input from the device.
    AudioInput,
    /// Audio output to the device.
    AudioOutput,
    /// MIDI input.
    MidiInput,
    /// MIDI output.
    MidiOutput,
    /// Meter tap for analysis.
    MeterTap,
}

/// Representation of a plugin chain inside the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginChain {
    /// Fixed size array of plugin slots.
    pub slots: [Option<PluginInstance>; 16],
    /// Indicates whether the chain is pre or post fader.
    pub pre_fader: bool,
}

impl Default for PluginChain {
    fn default() -> Self {
        Self {
            slots: Default::default(),
            pre_fader: true,
        }
    }
}

/// Information about an instantiated plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstance {
    /// Unique identifier for the instance.
    pub id: uuid::Uuid,
    /// Human readable name.
    pub name: String,
    /// Latency reported by the plugin.
    pub latency_samples: u32,
}

/// Connection between two pins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// Source pin.
    pub from: PinId,
    /// Destination pin.
    pub to: PinId,
    /// Linear gain applied to the routed signal.
    pub gain: f32,
}

/// Cached plugin delay compensation information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdcState {
    /// Latency accumulated per node.
    pub latency_per_node: Vec<u32>,
    /// Maximum latency across the graph.
    pub max_latency: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdc_propagates_latency() {
        let mut graph = Graph::new();
        let a = graph.add_node(NodeKind::AudioTrack, NodeParams::default());
        let b = graph.add_node(
            NodeKind::PluginContainer(PluginChain::default()),
            NodeParams::default(),
        );
        graph.set_latency(a, 64);
        graph.set_latency(b, 128);
        graph
            .connect(PinId { node: a, pin: 0 }, PinId { node: b, pin: 0 }, 1.0)
            .unwrap();
        graph.recompute_pdc();
        assert_eq!(graph.pdc.max_latency, 128);
    }
}
