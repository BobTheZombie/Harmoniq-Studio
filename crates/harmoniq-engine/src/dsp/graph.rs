use std::sync::Arc;

use parking_lot::Mutex;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

use harmoniq_dsp::{AudioBlock, AudioBlockMut};

use crate::dsp::events::{MidiEvent, Transport};
use crate::dsp::params::ParamUpdate;

pub type NodeId = u32;

#[derive(Clone, Copy, Debug, Default)]
pub struct NodeLatency {
    pub samples: u32,
}

#[derive(Clone)]
pub struct ParamPort {
    inner: Arc<Mutex<HeapProducer<ParamUpdate>>>,
}

impl ParamPort {
    #[inline]
    pub fn try_send(&self, update: ParamUpdate) -> Result<(), ParamUpdate> {
        let mut producer = self.inner.lock();
        producer.push(update)
    }

    #[inline]
    pub fn send(&self, update: ParamUpdate) {
        let _ = self.try_send(update);
    }
}

pub struct GraphProcess<'a> {
    pub inputs: AudioBlock<'a>,
    pub outputs: AudioBlockMut<'a>,
    pub frames: u32,
    pub transport: Transport,
    pub midi: &'a [MidiEvent],
}

pub struct ProcessContext<'a> {
    pub sr: f32,
    pub frames: u32,
    pub inputs: AudioBlock<'a>,
    pub outputs: AudioBlockMut<'a>,
    pub transport: Transport,
    pub midi: &'a [MidiEvent],
}

pub trait DspNode: Send {
    fn prepare(&mut self, sr: f32, max_block: u32, in_ch: u32, out_ch: u32);
    fn latency(&self) -> NodeLatency {
        NodeLatency { samples: 0 }
    }
    fn reset(&mut self) {}
    fn param(&mut self, update: ParamUpdate) {
        let _ = update;
    }
    fn process(&mut self, ctx: &mut ProcessContext<'_>);
}

struct NodeSlot {
    node: Box<dyn DspNode>,
    params: Option<HeapConsumer<ParamUpdate>>,
    latency: NodeLatency,
}

struct ParamPortInner {
    sender: Arc<Mutex<HeapProducer<ParamUpdate>>>,
}

enum BufferRef {
    Input,
    Scratch(usize),
    Output,
}

struct NodeExec {
    node_index: usize,
    input: BufferRef,
    output: BufferRef,
}

pub struct DspGraph {
    nodes: Vec<NodeSlot>,
    param_ports: Vec<Option<ParamPortInner>>,
    exec_order: Vec<NodeExec>,
    scratch: Vec<Vec<f32>>,
    sr: f32,
    max_block: u32,
    in_ch: u32,
    out_ch: u32,
    total_latency: u32,
}

impl DspGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            param_ports: Vec::new(),
            exec_order: Vec::new(),
            scratch: Vec::new(),
            sr: 44_100.0,
            max_block: 64,
            in_ch: 0,
            out_ch: 0,
            total_latency: 0,
        }
    }

    pub fn add_node(
        &mut self,
        node: Box<dyn DspNode>,
        param_capacity: usize,
    ) -> (NodeId, Option<ParamPort>) {
        let id = self.nodes.len() as NodeId;
        let mut consumer = None;
        let mut port = None;
        if param_capacity > 0 {
            let rb = HeapRb::new(param_capacity);
            let (producer, cons) = rb.split();
            let arc = Arc::new(Mutex::new(producer));
            consumer = Some(cons);
            port = Some(ParamPort { inner: arc.clone() });
            self.param_ports.push(Some(ParamPortInner { sender: arc }));
        } else {
            self.param_ports.push(None);
        }
        self.nodes.push(NodeSlot {
            node,
            params: consumer,
            latency: NodeLatency { samples: 0 },
        });
        (id, port)
    }

    pub fn set_topology(&mut self, order: &[NodeId]) {
        self.exec_order.clear();
        if order.is_empty() {
            self.scratch.clear();
            return;
        }
        let required = order.len().saturating_sub(1);
        self.resize_scratch(required);
        for (idx, node) in order.iter().enumerate() {
            let node_index = *node as usize;
            if node_index >= self.nodes.len() {
                continue;
            }
            let input = if idx == 0 {
                BufferRef::Input
            } else {
                BufferRef::Scratch(idx - 1)
            };
            let output = if idx == order.len() - 1 {
                BufferRef::Output
            } else {
                BufferRef::Scratch(idx)
            };
            self.exec_order.push(NodeExec {
                node_index,
                input,
                output,
            });
        }
    }

    pub fn prepare(&mut self, sr: f32, max_block: u32, in_ch: u32, out_ch: u32) {
        self.sr = sr;
        self.max_block = max_block.max(1);
        self.in_ch = in_ch;
        self.out_ch = out_ch;
        self.resize_scratch(self.exec_order.len().saturating_sub(1));
        self.total_latency = 0;
        for slot in &mut self.nodes {
            slot.node
                .prepare(self.sr, self.max_block, self.in_ch, self.out_ch);
            slot.latency = slot.node.latency();
            self.total_latency = self.total_latency.saturating_add(slot.latency.samples);
        }
    }

    pub fn total_latency(&self) -> NodeLatency {
        NodeLatency {
            samples: self.total_latency,
        }
    }

    pub fn param_port(&self, node: NodeId) -> Option<ParamPort> {
        let index = node as usize;
        self.param_ports.get(index).and_then(|slot| {
            slot.as_ref().map(|inner| ParamPort {
                inner: inner.sender.clone(),
            })
        })
    }

    pub fn process(&mut self, mut block: GraphProcess<'_>) {
        let frames = block.frames.min(self.max_block);
        if frames == 0 {
            return;
        }
        if self.exec_order.is_empty() {
            self.copy_block(block.inputs, &mut block.outputs, frames);
            return;
        }
        for exec in &self.exec_order {
            let node_slot = &mut self.nodes[exec.node_index];
            if let Some(params) = node_slot.params.as_mut() {
                while let Some(update) = params.pop() {
                    node_slot.node.param(update);
                }
            }
            let input_block = self.resolve_input(exec.input, block.inputs, frames);
            let mut output_block = self.resolve_output(exec.output, &mut block.outputs, frames);
            let mut ctx = ProcessContext {
                sr: self.sr,
                frames,
                inputs: input_block,
                outputs: output_block,
                transport: block.transport,
                midi: block.midi,
            };
            node_slot.node.process(&mut ctx);
        }
    }

    fn resolve_input<'a>(
        &'a self,
        source: BufferRef,
        input: AudioBlock<'a>,
        frames: u32,
    ) -> AudioBlock<'a> {
        match source {
            BufferRef::Input => input,
            BufferRef::Scratch(index) => {
                let frames = frames.min(self.max_block);
                if let Some(buf) = self.scratch.get(index) {
                    unsafe {
                        AudioBlock::from_interleaved(buf.as_ptr(), self.out_ch.max(1), frames)
                    }
                } else {
                    AudioBlock::empty()
                }
            }
            BufferRef::Output => input,
        }
    }

    fn resolve_output<'a>(
        &'a mut self,
        target: BufferRef,
        output: &mut AudioBlockMut<'a>,
        frames: u32,
    ) -> AudioBlockMut<'a> {
        match target {
            BufferRef::Input => AudioBlockMut::empty(),
            BufferRef::Scratch(index) => {
                let frames = frames.min(self.max_block);
                if let Some(buf) = self.scratch.get_mut(index) {
                    unsafe {
                        AudioBlockMut::from_interleaved(
                            buf.as_mut_ptr(),
                            self.out_ch.max(1),
                            frames,
                        )
                    }
                } else {
                    AudioBlockMut::empty()
                }
            }
            BufferRef::Output => {
                if output.is_interleaved() {
                    if let Some(ptr) = unsafe { output.interleaved_ptr_mut() } {
                        unsafe { AudioBlockMut::from_interleaved(ptr, self.out_ch.max(1), frames) }
                    } else {
                        AudioBlockMut::empty()
                    }
                } else if let Some(planes) = output.planes_ptrs_mut() {
                    unsafe { AudioBlockMut::from_planar(planes, self.out_ch.max(1), frames) }
                } else {
                    AudioBlockMut::empty()
                }
            }
        }
    }

    fn resize_scratch(&mut self, count: usize) {
        if self.scratch.len() < count {
            self.scratch.resize_with(count, Vec::new);
        } else if self.scratch.len() > count {
            self.scratch.truncate(count);
        }
        let required = self.block_samples();
        for buf in &mut self.scratch {
            if buf.len() != required {
                buf.resize(required, 0.0);
            }
        }
    }

    fn block_samples(&self) -> usize {
        let channels = self.out_ch.max(1) as usize;
        channels * self.max_block.max(1) as usize
    }

    fn copy_block(&self, src: AudioBlock<'_>, dst: &mut AudioBlockMut<'_>, frames: u32) {
        let frames = frames as usize;
        let channels = src.channels().min(dst.channels()) as usize;
        let dst_channels = dst.channels() as usize;
        for frame in 0..frames {
            for ch in 0..channels {
                let sample = unsafe { src.read_sample(ch, frame) };
                unsafe { dst.write_sample(ch, frame, sample) };
            }
            for ch in channels..dst_channels {
                unsafe { dst.write_sample(ch, frame, 0.0) };
            }
        }
    }
}
