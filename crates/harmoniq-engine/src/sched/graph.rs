use std::boxed::Box;
use std::vec::Vec;

pub type NodeId = u32;

#[derive(Debug)]
pub struct NodeMeta {
    pub id: NodeId,
    pub name: &'static str,
    pub latency: u32,
    pub tail: u32,
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
    pub order: Vec<NodeId>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            order: Vec::new(),
        }
    }
}

pub fn topo(g: &Graph) -> Vec<NodeId> {
    if g.order.is_empty() {
        (0..g.nodes.len() as NodeId).collect()
    } else {
        g.order.clone()
    }
}
