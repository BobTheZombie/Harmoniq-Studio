use std::collections::HashMap;
use std::sync::OnceLock;

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

    let mix_impl = *MIX_IMPLEMENTATION.get_or_init(detect_mix_impl);
    let channel_count = master.channel_count();

    for (index, node) in handle.plugin_nodes.iter().enumerate() {
        let gain = handle.gain_for(*node);
        if gain == 0.0 {
            continue;
        }

        if let Some(source) = sources.get(index) {
            let limit = channel_count.min(source.channel_count());

            for channel_index in 0..limit {
                let source_channel = source.channel(channel_index);
                let target_channel = master.channel_mut(channel_index);
                mix_channel_with_impl(target_channel, source_channel, gain, mix_impl);
            }
        }
    }
}

#[derive(Copy, Clone)]
enum MixImplementation {
    Scalar,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx2,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx512,
}

static MIX_IMPLEMENTATION: OnceLock<MixImplementation> = OnceLock::new();

fn detect_mix_impl() -> MixImplementation {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("avx512f") {
            return MixImplementation::Avx512;
        }
        if std::is_x86_feature_detected!("avx2") {
            return MixImplementation::Avx2;
        }
    }

    MixImplementation::Scalar
}

#[inline(always)]
fn mix_channel_with_impl(
    target: &mut [f32],
    source: &[f32],
    gain: f32,
    implementation: MixImplementation,
) {
    if gain == 0.0 {
        return;
    }

    let len = target.len().min(source.len());
    if len == 0 {
        return;
    }

    let (target_prefix, _) = target.split_at_mut(len);
    let source_prefix = &source[..len];

    match implementation {
        MixImplementation::Scalar => mix_channel_scalar(target_prefix, source_prefix, gain),
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        MixImplementation::Avx2 => unsafe { mix_channel_avx2(target_prefix, source_prefix, gain) },
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        MixImplementation::Avx512 => unsafe {
            mix_channel_avx512(target_prefix, source_prefix, gain)
        },
    }
}

#[inline(always)]
fn mix_channel_scalar(target: &mut [f32], source: &[f32], gain: f32) {
    for (dst, src) in target.iter_mut().zip(source.iter()) {
        *dst = src.mul_add(gain, *dst);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn mix_channel_avx2(target: &mut [f32], source: &[f32], gain: f32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let len = target.len();
    let ptr_target = target.as_mut_ptr();
    let ptr_source = source.as_ptr();
    let gain_vec = _mm256_set1_ps(gain);

    let mut offset = 0usize;
    while offset + 8 <= len {
        let src = _mm256_loadu_ps(ptr_source.add(offset));
        let dst = _mm256_loadu_ps(ptr_target.add(offset));
        let sum = _mm256_add_ps(dst, _mm256_mul_ps(src, gain_vec));
        _mm256_storeu_ps(ptr_target.add(offset), sum);
        offset += 8;
    }

    let remainder = len - offset;
    if remainder > 0 {
        let target_tail = std::slice::from_raw_parts_mut(ptr_target.add(offset), remainder);
        let source_tail = std::slice::from_raw_parts(ptr_source.add(offset), remainder);
        mix_channel_scalar(target_tail, source_tail, gain);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn mix_channel_avx512(target: &mut [f32], source: &[f32], gain: f32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let len = target.len();
    let ptr_target = target.as_mut_ptr();
    let ptr_source = source.as_ptr();
    let gain_vec = _mm512_set1_ps(gain);

    let mut offset = 0usize;
    while offset + 16 <= len {
        let src = _mm512_loadu_ps(ptr_source.add(offset));
        let dst = _mm512_loadu_ps(ptr_target.add(offset));
        let sum = _mm512_add_ps(dst, _mm512_mul_ps(src, gain_vec));
        _mm512_storeu_ps(ptr_target.add(offset), sum);
        offset += 16;
    }

    let remainder = len - offset;
    if remainder > 0 {
        let target_tail = std::slice::from_raw_parts_mut(ptr_target.add(offset), remainder);
        let source_tail = std::slice::from_raw_parts(ptr_source.add(offset), remainder);
        mix_channel_scalar(target_tail, source_tail, gain);
    }
}
