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

/// Owned planar audio buffer that can expose mutable slices per channel without
/// additional heap allocations. The buffer stores samples contiguously in
/// channel-major order so each channel remains cache friendly while still
/// allowing interleaved views to be produced from preallocated scratch space.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    data: Vec<f32>,
    channels: usize,
    frames: usize,
}

impl AudioBuffer {
    pub fn new(channels: usize, frames: usize) -> Self {
        let total = channels.saturating_mul(frames);
        Self {
            data: vec![0.0; total],
            channels,
            frames,
        }
    }

    pub fn from_config(config: &BufferConfig) -> Self {
        Self::new(config.layout.channels() as usize, config.block_size)
    }

    pub fn resize(&mut self, channels: usize, frames: usize) {
        let total = channels.saturating_mul(frames);
        if self.data.len() != total {
            self.data.resize(total, 0.0);
        }
        self.channels = channels;
        self.frames = frames;
    }

    pub fn clear(&mut self) {
        self.data.fill(0.0);
    }

    pub fn len(&self) -> usize {
        self.frames
    }

    pub fn is_empty(&self) -> bool {
        self.channels == 0 || self.frames == 0
    }

    pub fn channel_count(&self) -> usize {
        self.channels
    }

    pub fn channel(&self, index: usize) -> &[f32] {
        assert!(index < self.channels, "channel index out of bounds");
        let start = index * self.frames;
        &self.data[start..start + self.frames]
    }

    pub fn channel_mut(&mut self, index: usize) -> &mut [f32] {
        assert!(index < self.channels, "channel index out of bounds");
        let start = index * self.frames;
        &mut self.data[start..start + self.frames]
    }

    pub fn iter(&self) -> impl Iterator<Item = &f32> {
        self.data.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut f32> {
        self.data.iter_mut()
    }

    pub fn channels(&self) -> Channels<'_> {
        Channels {
            data: &self.data,
            frames: self.frames,
            channels: self.channels,
            index: 0,
        }
    }

    pub fn channels_mut(&mut self) -> ChannelsMut<'_> {
        ChannelsMut {
            data: self.data.as_mut_ptr(),
            frames: self.frames,
            channels: self.channels,
            index: 0,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::new(1, 1)
    }
}

pub struct Channels<'a> {
    data: &'a [f32],
    frames: usize,
    channels: usize,
    index: usize,
}

impl<'a> Iterator for Channels<'a> {
    type Item = &'a [f32];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.channels {
            return None;
        }
        let start = self.index * self.frames;
        let end = start + self.frames;
        self.index += 1;
        Some(&self.data[start..end])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.channels.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for Channels<'a> {}

pub struct ChannelsMut<'a> {
    data: *mut f32,
    frames: usize,
    channels: usize,
    index: usize,
    _marker: std::marker::PhantomData<&'a mut f32>,
}

impl<'a> Iterator for ChannelsMut<'a> {
    type Item = &'a mut [f32];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.channels {
            return None;
        }
        let start = self.index * self.frames;
        let end = start + self.frames;
        self.index += 1;
        if self.frames == 0 {
            return Some(unsafe { std::slice::from_raw_parts_mut(self.data.add(start), 0) });
        }
        let slice = unsafe { std::slice::from_raw_parts_mut(self.data.add(start), end - start) };
        Some(slice)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.channels.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for ChannelsMut<'a> {}
