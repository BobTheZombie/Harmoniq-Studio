use std::collections::HashMap;

use crate::{plugin::PluginId, AudioBuffer};

/// Node identifier within a processing graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHandle(pub u32);

/// A fully prepared processing graph ready to be executed by the engine.
#[derive(Debug, Clone)]
pub struct GraphHandle {
    pub(crate) nodes: Vec<PluginId>,
    pub(crate) mixer_gains: HashMap<NodeHandle, f32>,
}

impl GraphHandle {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Helper builder for declaring processor topologies.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    nodes: Vec<PluginId>,
    mixer_gains: HashMap<NodeHandle, f32>,
    next_id: u32,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, plugin: PluginId) -> NodeHandle {
        let id = NodeHandle(self.next_id);
        self.next_id += 1;
        self.nodes.push(plugin);
        id
    }

    pub fn connect_to_mixer(&mut self, node: NodeHandle, gain: f32) -> anyhow::Result<()> {
        if gain < 0.0 {
            anyhow::bail!("Gain must be non-negative");
        }
        if node.0 >= self.next_id {
            anyhow::bail!("Unknown node: {}", node.0);
        }
        self.mixer_gains.insert(node, gain);
        Ok(())
    }

    pub fn build(self) -> GraphHandle {
        GraphHandle {
            nodes: self.nodes,
            mixer_gains: self.mixer_gains,
        }
    }
}

/// Simple stereo mixer that sums node outputs into the master buffer.
pub(crate) fn mixdown(
    buffer: &mut AudioBuffer,
    sources: &[AudioBuffer],
    mixer_gains: &HashMap<NodeHandle, f32>,
) {
    buffer.clear();
    for (index, source) in sources.iter().enumerate() {
        let handle = NodeHandle(index as u32);
        let gain = mixer_gains.get(&handle).copied().unwrap_or(1.0);
        for (target_channel, source_channel) in buffer.channels_mut().zip(source.channels()) {
            for (target_sample, source_sample) in target_channel.iter_mut().zip(source_channel) {
                *target_sample += source_sample * gain;
            }
        }
    }
}
