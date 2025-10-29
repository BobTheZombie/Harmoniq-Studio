use anyhow::Result;

use crate::buffers::{AudioView, AudioViewMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamConfig {
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub in_channels: u16,
    pub out_channels: u16,
    pub interleaved: bool,
}

impl StreamConfig {
    pub fn new(
        sample_rate: u32,
        buffer_frames: u32,
        in_channels: u16,
        out_channels: u16,
        interleaved: bool,
    ) -> Self {
        Self {
            sample_rate,
            buffer_frames,
            in_channels,
            out_channels,
            interleaved,
        }
    }
}

pub trait EngineRt: Send {
    fn process(&mut self, inputs: AudioView<'_>, outputs: AudioViewMut<'_>, frames: u32) -> bool;
}

pub trait AudioBackend {
    fn start(&mut self, rt: Box<dyn EngineRt>) -> Result<()>;
    fn stop(&mut self);
}

pub mod safety;

#[cfg(all(target_os = "linux", feature = "openasio_gpl"))]
pub mod openasio;
