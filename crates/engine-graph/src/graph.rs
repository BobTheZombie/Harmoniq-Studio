use crate::automation::{ParameterSet, ParameterView};
use crate::node::{AudioNode, NodePreparation, PortBuffer, ProcessContext};
use crate::NodeId;
use anyhow::{anyhow, Result};
use engine_rt::transport::TransportState;
use std::collections::{HashMap, VecDeque};

pub struct GraphConfig {
    pub sample_rate: u32,
    pub block_size: usize,
    pub channels: usize,
}

impl GraphConfig {
    pub fn new(sample_rate: u32, block_size: usize, channels: usize) -> Self {
        Self {
            sample_rate,
            block_size,
            channels,
        }
    }
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            block_size: 256,
            channels: 2,
        }
    }
}

#[derive(Clone)]
struct ConnectionTarget {
    node: NodeId,
}

struct NodeSpec {
    id: NodeId,
    node: Box<dyn AudioNode>,
    inputs: Vec<Option<ConnectionTarget>>,
    outputs: usize,
}

pub struct GraphBuilder {
    nodes: Vec<NodeSpec>,
    next_id: NodeId,
    output: Option<NodeId>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_id: 1,
            output: None,
        }
    }

    pub fn add_node(
        &mut self,
        node: Box<dyn AudioNode>,
        input_ports: usize,
        output_ports: usize,
    ) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(NodeSpec {
            id,
            node,
            inputs: vec![None; input_ports],
            outputs: output_ports.max(1),
        });
        id
    }

    pub fn connect(
        &mut self,
        from: NodeId,
        _from_port: usize,
        to: NodeId,
        to_port: usize,
    ) -> Result<()> {
        let target = ConnectionTarget { node: from };
        let node = self
            .nodes
            .iter_mut()
            .find(|spec| spec.id == to)
            .ok_or_else(|| anyhow!("unknown destination node {to}"))?;
        if to_port >= node.inputs.len() {
            return Err(anyhow!("destination port {to_port} out of range"));
        }
        node.inputs[to_port] = Some(target);
        Ok(())
    }

    pub fn designate_output(&mut self, node: NodeId) {
        self.output = Some(node);
    }

    pub fn build(self) -> Result<AudioGraph> {
        let output = self
            .output
            .ok_or_else(|| anyhow!("no output node designated"))?;
        Ok(AudioGraph {
            nodes: self.nodes,
            output_node: output,
            parameters: ParameterSet::new(),
        })
    }
}

pub struct AudioGraph {
    nodes: Vec<NodeSpec>,
    output_node: NodeId,
    parameters: ParameterSet,
}

impl AudioGraph {
    pub fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    pub fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    pub fn into_executor(self, config: GraphConfig) -> Result<GraphExecutor> {
        GraphExecutor::from_graph(self, config)
    }
}

struct NodeRuntime {
    id: NodeId,
    node: Box<dyn AudioNode>,
    inputs: Vec<Option<usize>>,
    input_buffers: Vec<PortBuffer>,
    output_buffers: Vec<PortBuffer>,
    latency_samples: usize,
}

impl NodeRuntime {
    fn new(
        spec: NodeSpec,
        index_map: &HashMap<NodeId, usize>,
        config: &GraphConfig,
    ) -> Result<Self> {
        let inputs = spec
            .inputs
            .iter()
            .map(|target| match target {
                Some(target) => index_map
                    .get(&target.node)
                    .copied()
                    .ok_or_else(|| anyhow!("dangling connection to node {}", target.node))
                    .map(Some),
                None => Ok(None),
            })
            .collect::<Result<Vec<_>>>()?;

        let input_buffers = inputs
            .iter()
            .map(|_| PortBuffer::new(config.channels, config.block_size))
            .collect::<Vec<_>>();
        let output_buffers = (0..spec.outputs)
            .map(|_| PortBuffer::new(config.channels, config.block_size))
            .collect::<Vec<_>>();

        Ok(Self {
            id: spec.id,
            node: spec.node,
            inputs,
            input_buffers,
            output_buffers,
            latency_samples: 0,
        })
    }
}

pub struct GraphExecutor {
    nodes: Vec<NodeRuntime>,
    order: Vec<usize>,
    output_index: usize,
    config: GraphConfig,
    parameters: ParameterSet,
    latency_samples: usize,
}

impl GraphExecutor {
    fn from_graph(graph: AudioGraph, config: GraphConfig) -> Result<Self> {
        let mut index_map = HashMap::new();
        for (index, spec) in graph.nodes.iter().enumerate() {
            index_map.insert(spec.id, index);
        }

        let mut adjacency = vec![Vec::new(); graph.nodes.len()];
        let mut indegree = vec![0usize; graph.nodes.len()];

        for (index, spec) in graph.nodes.iter().enumerate() {
            for input in &spec.inputs {
                if let Some(target) = input {
                    let source_index = *index_map
                        .get(&target.node)
                        .ok_or_else(|| anyhow!("unknown connection source {}", target.node))?;
                    adjacency[source_index].push(index);
                    indegree[index] += 1;
                }
            }
        }

        let mut queue = VecDeque::new();
        for (index, &degree) in indegree.iter().enumerate() {
            if degree == 0 {
                queue.push_back(index);
            }
        }

        let mut order = Vec::with_capacity(graph.nodes.len());
        while let Some(index) = queue.pop_front() {
            order.push(index);
            for &next in &adjacency[index] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }

        if order.len() != graph.nodes.len() {
            return Err(anyhow!("graph contains a cycle"));
        }

        let output_index = *index_map
            .get(&graph.output_node)
            .ok_or_else(|| anyhow!("output node {} missing", graph.output_node))?;

        let mut nodes = Vec::with_capacity(graph.nodes.len());
        for spec in graph.nodes.into_iter() {
            let mut runtime = NodeRuntime::new(spec, &index_map, &config)?;
            runtime.node.prepare(&NodePreparation {
                sample_rate: config.sample_rate as f32,
                block_size: config.block_size,
                channels: config.channels,
            });
            runtime.latency_samples = runtime.node.latency_samples();
            nodes.push(runtime);
        }

        let latency_samples = nodes.iter().map(|node| node.latency_samples).sum();

        Ok(Self {
            nodes,
            order,
            output_index,
            config,
            parameters: graph.parameters,
            latency_samples,
        })
    }

    pub fn process(&mut self, output: &mut [f32], transport: &TransportState) {
        let frames = self.config.block_size;
        let channels = self.config.channels;
        for &node_index in &self.order {
            let (before, rest) = self.nodes.split_at_mut(node_index);
            let (node, _) = rest.split_first_mut().expect("valid index");

            for (port_index, source) in node.inputs.iter().enumerate() {
                let buffer = &mut node.input_buffers[port_index];
                buffer.clear();
                if let Some(source_index) = source {
                    if *source_index < node_index {
                        let source_node = &before[*source_index];
                        if let Some(source_port) = source_node.output_buffers.first() {
                            buffer.copy_from(source_port);
                        }
                    }
                }
            }

            for port in &mut node.output_buffers {
                port.clear();
            }

            let parameter_view = ParameterView::new(&self.parameters);
            let mut context = ProcessContext {
                node_id: node.id,
                sample_rate: self.config.sample_rate as f32,
                frames,
                transport,
                parameters: parameter_view,
            };
            node.node
                .process(&node.input_buffers, &mut node.output_buffers, &mut context);
        }

        let Some(final_output) = self.nodes[self.output_index].output_buffers.first() else {
            return;
        };

        for frame in 0..frames {
            for channel in 0..channels {
                let sample = final_output.channel(channel)[frame];
                let index = frame * channels + channel;
                if index < output.len() {
                    output[index] = sample;
                }
            }
        }
    }

    pub fn latency_samples(&self) -> usize {
        self.latency_samples
    }

    pub fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn block_size(&self) -> usize {
        self.config.block_size
    }

    pub fn channels(&self) -> usize {
        self.config.channels
    }
}
