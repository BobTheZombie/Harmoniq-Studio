use serde::{Deserialize, Serialize};

/// Represents the desired output channel configuration for a processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Surround51,
    Custom(u8),
}

impl ChannelLayout {
    pub fn channels(&self) -> u8 {
        match self {
            ChannelLayout::Mono => 1,
            ChannelLayout::Stereo => 2,
            ChannelLayout::Surround51 => 6,
            ChannelLayout::Custom(channels) => *channels,
        }
    }
}

/// Shared configuration passed to processors during preparation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BufferConfig {
    pub sample_rate: f32,
    pub block_size: usize,
    pub layout: ChannelLayout,
}

impl BufferConfig {
    pub fn new(sample_rate: f32, block_size: usize, layout: ChannelLayout) -> Self {
        Self {
            sample_rate,
            block_size,
            layout,
        }
    }
}

/// Non-interleaved audio buffer for processing.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    channels: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn new(num_channels: usize, block_size: usize) -> Self {
        let channels = (0..num_channels).map(|_| vec![0.0; block_size]).collect();
        Self { channels }
    }

    pub fn from_config(config: BufferConfig) -> Self {
        Self::new(config.layout.channels() as usize, config.block_size)
    }

    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            channel.fill(0.0);
        }
    }

    pub fn len(&self) -> usize {
        self.channels
            .first()
            .map(|channel| channel.len())
            .unwrap_or_default()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut f32> {
        self.channels
            .iter_mut()
            .flat_map(|channel| channel.iter_mut())
    }

    pub fn channels(&self) -> impl Iterator<Item = &Vec<f32>> {
        self.channels.iter()
    }

    pub fn channels_mut(&mut self) -> impl Iterator<Item = &mut Vec<f32>> {
        self.channels.iter_mut()
    }

    pub fn as_slice(&self) -> &[Vec<f32>] {
        &self.channels
    }

    pub fn as_mut_slice(&mut self) -> &mut [Vec<f32>] {
        &mut self.channels
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self {
            channels: vec![vec![0.0; 1]],
        }
    }
}
