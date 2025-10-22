use crate::automation::ParameterView;
use crate::NodeId;
use engine_rt::transport::TransportState;

/// Information provided when preparing a node for execution.
pub struct NodePreparation {
    pub sample_rate: f32,
    pub block_size: usize,
    pub channels: usize,
}

/// Read-only view over a multi-channel audio port.
#[derive(Debug)]
pub struct PortBuffer {
    channels: Vec<Vec<f32>>,
}

impl PortBuffer {
    pub fn new(channels: usize, frames: usize) -> Self {
        let channels = (0..channels)
            .map(|_| vec![0.0_f32; frames])
            .collect::<Vec<_>>();
        Self { channels }
    }

    pub fn channels(&self) -> usize {
        self.channels.len()
    }

    pub fn channel(&self, index: usize) -> &[f32] {
        &self.channels[index]
    }

    pub fn channel_mut(&mut self, index: usize) -> &mut [f32] {
        &mut self.channels[index]
    }

    pub fn frames(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or_default()
    }

    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            channel.fill(0.0);
        }
    }

    pub fn copy_from(&mut self, other: &PortBuffer) {
        for (dst, src) in self.channels.iter_mut().zip(&other.channels) {
            dst.copy_from_slice(src);
        }
    }
}

/// Execution context delivered to an [`AudioNode`].
pub struct ProcessContext<'a> {
    pub node_id: NodeId,
    pub sample_rate: f32,
    pub frames: usize,
    pub transport: &'a TransportState,
    pub parameters: ParameterView<'a>,
}

pub trait AudioNode: Send {
    fn prepare(&mut self, preparation: &NodePreparation);
    fn process(
        &mut self,
        inputs: &[PortBuffer],
        outputs: &mut [PortBuffer],
        context: &ProcessContext<'_>,
    );
    fn latency_samples(&self) -> usize {
        0
    }
}
