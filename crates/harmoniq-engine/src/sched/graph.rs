use std::boxed::Box;
use std::vec::Vec;

pub type NodeId = u32;

#[derive(Debug)]
pub struct NodeMeta {
    pub id: NodeId,
    pub name: &'static str,
    pub latency: u32,
    pub tail: u32,
    pub parallel_safe: bool,
}

pub trait Node: Send {
    fn meta(&self) -> &NodeMeta;
    fn prepare(&mut self, sr: u32, max_block: u32);
    fn process(
        &mut self,
        bufs: &mut crate::sched::buffer::AudioBuffers,
        ev: &crate::sched::events::EventSlice,
    );
}

pub struct Graph {
    pub nodes: Vec<Box<dyn Node>>,
    pub topo: Vec<NodeId>,
    pub depths: Vec<(usize, usize)>,
    pub parallel_safe: Vec<bool>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            topo: Vec::new(),
            depths: Vec::new(),
            parallel_safe: Vec::new(),
        }
    }
}

pub fn build(g: &mut Graph) {
    let node_count = g.nodes.len();
    if g.topo.len() != node_count {
        g.topo.clear();
        g.topo.extend(0..node_count as u32);
    }

    g.parallel_safe.clear();
    g.parallel_safe
        .extend(g.nodes.iter().map(|node| node.meta().parallel_safe));

    g.depths.clear();
    if !g.topo.is_empty() {
        g.depths.push((0, g.topo.len()));
    }
}
