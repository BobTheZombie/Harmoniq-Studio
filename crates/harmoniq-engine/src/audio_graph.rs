use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use crate::automation::AutomationEvent;
use crate::buffer::AudioBuffer;
use crate::delay::DelayCompensator;
use crate::mixer_rt::{Mixer, MixerConfig};
use crate::plugin::{MidiEvent, PluginId};
use crate::AudioProcessor;

/// Real-time friendly DSP node abstraction used by the audio graph runner.
pub trait DspNode: Send {
    /// Maximum latency reported by the node in samples.
    fn latency(&self) -> usize {
        0
    }

    /// Process the node for the current block.
    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        output: &mut AudioBuffer,
        frames: usize,
    ) -> anyhow::Result<()>;
}

struct NodeSpec {
    node: Box<dyn DspNode + Send>,
    inputs: Vec<usize>,
}

struct NodeState {
    spec: NodeSpec,
    buffer: AudioBuffer,
}

/// Pre-topologized DAG prepared outside the audio thread and executed as a pull graph.
pub struct GraphRunner {
    nodes: Vec<NodeState>,
    order: Vec<usize>,
    master_index: usize,
    channels: usize,
    max_block: usize,
}

impl GraphRunner {
    pub fn new(
        nodes: Vec<NodeSpec>,
        master_index: usize,
        channels: usize,
        max_block: usize,
    ) -> Self {
        let mut state: Vec<NodeState> = nodes
            .into_iter()
            .map(|spec| NodeState {
                spec,
                buffer: AudioBuffer::new(channels, max_block),
            })
            .collect();

        for node in &mut state {
            node.buffer.resize(channels, max_block);
        }

        let order = (0..state.len()).collect();

        Self {
            nodes: state,
            order,
            master_index,
            channels,
            max_block: max_block.max(1),
        }
    }

    pub fn master(&self) -> &AudioBuffer {
        &self.nodes[self.master_index].buffer
    }

    pub fn master_mut(&mut self) -> &mut AudioBuffer {
        &mut self.nodes[self.master_index].buffer
    }

    pub fn node_outputs(&self) -> Vec<&AudioBuffer> {
        self.nodes.iter().map(|node| &node.buffer).collect()
    }

    pub fn process(&mut self, frames: usize) -> anyhow::Result<()> {
        let frames = frames.min(self.max_block);
        if frames == 0 {
            return Ok(());
        }

        for index in &self.order {
            let (before, after) = self.nodes.split_at_mut(*index);
            let (node, after) = after.split_first_mut().expect("index must be valid");

            let inputs: Vec<&AudioBuffer> = node
                .spec
                .inputs
                .iter()
                .filter_map(|idx| {
                    if *idx < *index {
                        before.get(*idx)
                    } else if *idx > *index {
                        after.get(idx - index - 1)
                    } else {
                        None
                    }
                })
                .map(|node| &node.buffer)
                .collect();

            node.buffer.resize(self.channels, frames);
            node.buffer.clear();
            node.spec.node.process(&inputs, &mut node.buffer, frames)?;
        }

        Ok(())
    }
}

/// Node that wraps an [`AudioProcessor`] instrument or effect instance.
pub struct ProcessorNode {
    processor: Arc<Mutex<Box<dyn AudioProcessor>>>,
    automation: Vec<AutomationEvent>,
    midi: Vec<MidiEvent>,
    latency: usize,
}

impl ProcessorNode {
    pub fn new(
        processor: Arc<Mutex<Box<dyn AudioProcessor>>>,
        automation: Vec<AutomationEvent>,
        midi: Vec<MidiEvent>,
        latency: usize,
    ) -> Self {
        Self {
            processor,
            automation,
            midi,
            latency,
        }
    }
}

impl DspNode for ProcessorNode {
    fn latency(&self) -> usize {
        self.latency
    }

    fn process(
        &mut self,
        _inputs: &[&AudioBuffer],
        output: &mut AudioBuffer,
        _frames: usize,
    ) -> anyhow::Result<()> {
        if output.channel_count() == 0 || output.len() == 0 {
            return Ok(());
        }

        let mut guard = self.processor.lock();

        for event in &self.automation {
            guard.handle_automation_event(
                event.parameter,
                event.value,
                event.sample_offset as usize,
            )?;
        }

        if !self.midi.is_empty() {
            guard.process_midi(&self.midi)?;
        }

        guard.process(output)
    }
}

/// Per-node delay compensator that reuses a stable allocation stored on the engine.
pub struct DelayNode {
    delay: NonNull<DelayCompensator>,
    additional: usize,
    channels: usize,
    block_size: usize,
}

unsafe impl Send for DelayNode {}

impl DelayNode {
    pub fn new(
        delay: NonNull<DelayCompensator>,
        additional: usize,
        channels: usize,
        block_size: usize,
    ) -> Self {
        Self {
            delay,
            additional,
            channels,
            block_size,
        }
    }
}

impl DspNode for DelayNode {
    fn latency(&self) -> usize {
        self.additional
    }

    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        output: &mut AudioBuffer,
        frames: usize,
    ) -> anyhow::Result<()> {
        if let Some(input) = inputs.first() {
            if output.channel_count() != input.channel_count() || output.len() != frames {
                output.resize(input.channel_count(), frames);
            }
            let src = input.as_slice();
            let dst = output.as_mut_slice();
            let len = dst.len().min(src.len());
            dst[..len].copy_from_slice(&src[..len]);
        }

        if self.additional > 0 {
            // SAFETY: the pointer originates from a stable Box stored on the engine.
            let delay = unsafe { self.delay.as_mut() };
            delay.configure(self.channels, self.additional, self.block_size);
            delay.process(output);
        }

        Ok(())
    }
}

/// Mix node that sums track buffers into the master bus.
pub struct MixerNode {
    mixer: NonNull<Mixer>,
    cfg: MixerConfig,
}

unsafe impl Send for MixerNode {}

impl MixerNode {
    pub fn new(mixer: NonNull<Mixer>, cfg: MixerConfig) -> Self {
        Self { mixer, cfg }
    }
}

impl DspNode for MixerNode {
    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        output: &mut AudioBuffer,
        frames: usize,
    ) -> anyhow::Result<()> {
        let frames = frames.min(self.cfg.max_block);
        if frames == 0 {
            output.clear();
            return Ok(());
        }

        let mixer = unsafe { self.mixer.as_mut() };
        mixer.begin_block();

        let mut input_slices: Vec<Option<&[f32]>> = Vec::with_capacity(inputs.len());
        for buffer in inputs.iter().take(self.cfg.max_tracks) {
            if buffer.channel_count() == 0 || buffer.len() == 0 {
                input_slices.push(None);
            } else {
                input_slices.push(Some(buffer.channel(0)));
            }
        }

        if output.channel_count() != 2 || output.len() != frames {
            output.resize(2, frames);
        }

        let (out_l, out_r) = {
            let data = output.as_mut_slice();
            let (left, rest) = data.split_at_mut(frames);
            let (right, _) = rest.split_at_mut(frames.min(rest.len()));
            (left, right)
        };

        mixer.process(&input_slices, out_l, out_r, frames);
        mixer.end_block();
        Ok(())
    }
}

/// Helper to assemble the pre-topologized graph for the current block.
pub fn build_graph(
    plugin_ids: &[PluginId],
    processors: &[Arc<Mutex<Box<dyn AudioProcessor>>>],
    latencies: &[usize],
    automation: &[Vec<AutomationEvent>],
    midi: &[MidiEvent],
    mixer: NonNull<Mixer>,
    mixer_cfg: MixerConfig,
    delay_lines: &mut HashMap<PluginId, Box<DelayCompensator>>,
    channels: usize,
    block_size: usize,
) -> GraphRunner {
    let max_latency = latencies.iter().copied().max().unwrap_or(0);

    let mut nodes: Vec<NodeSpec> = Vec::new();
    let mut mixer_inputs = Vec::new();

    for (index, (plugin_id, processor)) in plugin_ids.iter().zip(processors.iter()).enumerate() {
        let automation_bucket = automation.get(index).cloned().unwrap_or_default();
        let latency = *latencies.get(index).unwrap_or(&0);
        let proc_idx = nodes.len();
        nodes.push(NodeSpec {
            node: Box::new(ProcessorNode::new(
                Arc::clone(processor),
                automation_bucket,
                midi.to_vec(),
                latency,
            )),
            inputs: Vec::new(),
        });

        let extra_delay = max_latency.saturating_sub(latency);
        let final_idx = if extra_delay > 0 {
            let entry = delay_lines
                .entry(*plugin_id)
                .or_insert_with(|| Box::new(DelayCompensator::new()));
            let ptr = NonNull::from(entry.as_mut());
            let idx = nodes.len();
            nodes.push(NodeSpec {
                node: Box::new(DelayNode::new(ptr, extra_delay, channels, block_size)),
                inputs: vec![proc_idx],
            });
            idx
        } else {
            if let Some(delay) = delay_lines.get_mut(plugin_id) {
                delay.reset();
            }
            proc_idx
        };

        mixer_inputs.push(final_idx);
    }

    let master_index = nodes.len();
    nodes.push(NodeSpec {
        node: Box::new(MixerNode::new(mixer, mixer_cfg)),
        inputs: mixer_inputs,
    });

    GraphRunner::new(nodes, master_index, channels, block_size)
}
