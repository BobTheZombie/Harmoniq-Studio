use std::ops::{Index, IndexMut};

/// Lightweight audio buffer used for external plugin hosting.
///
/// The buffer stores channel-major audio data and exposes helpers for
/// allocating, clearing, and resizing the buffer in a real-time friendly
/// manner. The container is intentionally simple so that host
/// implementations for different plugin formats can share the same data
/// structure without needing to depend on the Harmoniq engine crate.
#[derive(Clone, Debug)]
pub struct AudioBuffer {
    channels: Vec<Vec<f32>>,
    frames: usize,
}

impl AudioBuffer {
    /// Creates a buffer with the provided number of channels and frames,
    /// initialised to silence.
    pub fn new(channels: usize, frames: usize) -> Self {
        let mut data = Vec::with_capacity(channels);
        for _ in 0..channels {
            data.push(vec![0.0; frames]);
        }
        Self {
            channels: data,
            frames,
        }
    }

    /// Returns the number of channels stored in the buffer.
    pub fn channels(&self) -> usize {
        self.channels.len()
    }

    /// Returns the number of sample frames in the buffer.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Clears the contents of the buffer back to silence.
    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            for sample in channel.iter_mut() {
                *sample = 0.0;
            }
        }
    }

    /// Resizes the buffer while preserving existing contents where
    /// possible. New samples are initialised to silence.
    pub fn resize(&mut self, channels: usize, frames: usize) {
        if self.channels.len() != channels {
            self.channels.resize_with(channels, Vec::new);
        }
        for channel in &mut self.channels {
            channel.resize(frames, 0.0);
        }
        self.frames = frames;
    }

    /// Returns an iterator over channel slices.
    pub fn channel_slices(&self) -> impl Iterator<Item = &[f32]> {
        self.channels.iter().map(|channel| channel.as_slice())
    }

    /// Returns an iterator over mutable channel slices.
    pub fn channel_slices_mut(&mut self) -> impl Iterator<Item = &mut [f32]> {
        self.channels
            .iter_mut()
            .map(|channel| channel.as_mut_slice())
    }

    /// Copies samples from another buffer into this one. Channel counts and
    /// frame lengths must match.
    pub fn copy_from(&mut self, other: &AudioBuffer) {
        assert_eq!(self.frames, other.frames);
        assert_eq!(self.channels.len(), other.channels.len());
        for (dst, src) in self.channels.iter_mut().zip(other.channels.iter()) {
            dst.copy_from_slice(src);
        }
    }
}

impl Index<usize> for AudioBuffer {
    type Output = [f32];

    fn index(&self, index: usize) -> &Self::Output {
        self.channels[index].as_slice()
    }
}

impl IndexMut<usize> for AudioBuffer {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.channels[index].as_mut_slice()
    }
}
