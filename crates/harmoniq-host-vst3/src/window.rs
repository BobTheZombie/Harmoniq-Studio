use anyhow::{bail, Result};

/// Abstraction for embedding plugin UI windows inside the Harmoniq shell application.
pub trait WindowEmbedder {
    fn attach(&self, window_id: u64) -> Result<()>;
    fn detach(&self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct WaylandEmbedder;

impl WindowEmbedder for WaylandEmbedder {
    fn attach(&self, _window_id: u64) -> Result<()> {
        bail!("Wayland embedding is not yet implemented")
    }

    fn detach(&self) -> Result<()> {
        bail!("Wayland embedding is not yet implemented")
    }
}

#[derive(Debug, Default)]
pub struct X11Embedder;

impl WindowEmbedder for X11Embedder {
    fn attach(&self, _window_id: u64) -> Result<()> {
        bail!("X11 embedding is not yet implemented")
    }

    fn detach(&self) -> Result<()> {
        bail!("X11 embedding is not yet implemented")
    }
}
