use std::collections::HashMap;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};

use crate::{plugin::PluginId, AudioBuffer};

/// Node identifier within a processing graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHandle(pub(crate) NodeIndex);

#[derive(Debug, Clone)]
pub enum NodeKind {
    Input,
    Plugin { id: PluginId },
    MixerBus { name: String },
    Master,
}

#[derive(Debug, Clone, Copy)]
pub struct Connection {
    pub gain: f32,
}

/// A fully prepared processing graph ready to be executed by the engine.
#[derive(Debug, Clone)]
pub struct GraphHandle {
    pub(crate) graph: StableDiGraph<NodeKind, Connection>,
    pub(crate) master: NodeIndex,
    pub(crate) plugin_nodes: Vec<NodeIndex>,
    pub(crate) node_lookup: HashMap<NodeIndex, usize>,
}

impl GraphHandle {
    pub fn is_empty(&self) -> bool {
        self.plugin_nodes.is_empty()
    }

    pub fn plugin_ids(&self) -> Vec<PluginId> {
        self.plugin_nodes
            .iter()
            .filter_map(|index| match &self.graph[*index] {
                NodeKind::Plugin { id } => Some(*id),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn plugin_nodes(&self) -> &[NodeIndex] {
        &self.plugin_nodes
    }

    pub(crate) fn gain_for(&self, node: NodeIndex) -> f32 {
        if let Some(edge) = self.graph.find_edge(node, self.master) {
            self.graph[edge].gain
        } else {
            1.0
        }
    }
}

/// Helper builder for declaring processor topologies.
#[derive(Debug)]
pub struct GraphBuilder {
    graph: StableDiGraph<NodeKind, Connection>,
    master: NodeIndex,
    plugin_nodes: Vec<NodeIndex>,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        let mut graph = StableDiGraph::new();
        let master = graph.add_node(NodeKind::Master);
        Self {
            graph,
            master,
            plugin_nodes: Vec::new(),
        }
    }
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_input(&mut self) -> NodeHandle {
        NodeHandle(self.graph.add_node(NodeKind::Input))
    }

    pub fn add_mixer_bus(&mut self, name: impl Into<String>) -> NodeHandle {
        NodeHandle(
            self.graph
                .add_node(NodeKind::MixerBus { name: name.into() }),
        )
    }

    pub fn add_node(&mut self, plugin: PluginId) -> NodeHandle {
        let node = self.graph.add_node(NodeKind::Plugin { id: plugin });
        self.plugin_nodes.push(node);
        NodeHandle(node)
    }

    pub fn connect(&mut self, from: NodeHandle, to: NodeHandle, gain: f32) -> anyhow::Result<()> {
        if gain < 0.0 {
            anyhow::bail!("Gain must be non-negative");
        }
        self.graph.add_edge(from.0, to.0, Connection { gain });
        Ok(())
    }

    pub fn connect_to_mixer(&mut self, node: NodeHandle, gain: f32) -> anyhow::Result<()> {
        if gain < 0.0 {
            anyhow::bail!("Gain must be non-negative");
        }
        self.graph
            .add_edge(node.0, self.master, Connection { gain });
        Ok(())
    }

    pub fn build(self) -> GraphHandle {
        let mut node_lookup = HashMap::new();
        for (index, node) in self.plugin_nodes.iter().enumerate() {
            node_lookup.insert(*node, index);
        }
        GraphHandle {
            graph: self.graph,
            master: self.master,
            plugin_nodes: self.plugin_nodes,
            node_lookup,
        }
    }
}

/// Simple stereo mixer that sums node outputs into the master buffer.
pub(crate) fn mixdown(handle: &GraphHandle, master: &mut AudioBuffer, sources: &[AudioBuffer]) {
    master.clear();
    for (index, node) in handle.plugin_nodes.iter().enumerate() {
        let gain = handle.gain_for(*node);
        if let Some(source) = sources.get(index) {
            for (target_channel, source_channel) in master.channels_mut().zip(source.channels()) {
                for (target_sample, source_sample) in
                    target_channel.iter_mut().zip(source_channel.iter())
                {
                    *target_sample += source_sample * gain;
                }
            }
        }
    }
}
