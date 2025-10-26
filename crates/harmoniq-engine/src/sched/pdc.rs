use crate::sched::graph::Node;

pub fn pdc_offset_for(node: &dyn Node) -> i32 {
    node.meta().latency as i32
}
