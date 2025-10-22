//! IO backends provide access to audio hardware.

use engine_rt::CallbackHandle;
#[cfg(feature = "cpal")]
use engine_rt::{AudioCallbackInfo, InterleavedAudioBuffer};
use thiserror::Error;

#[cfg(feature = "cpal")]
pub mod cpal_backend;

#[cfg(not(feature = "cpal"))]
pub mod cpal_backend {
    use super::*;

    #[derive(Default, Clone)]
    pub struct CpalBackend;

    impl CpalBackend {
        pub fn new() -> Self {
            Self
        }
    }

    impl AudioBackend for CpalBackend {
        fn name(&self) -> &'static str {
            "cpal (stub)"
        }

        fn devices(&self) -> Result<Vec<DeviceInfo>> {
            Ok(Vec::new())
        }

        fn default_output(&self) -> Result<DeviceId> {
            Err(BackendError::Backend(
                "cpal backend not available in this build".into(),
            ))
        }

        fn open_output_stream(
            &self,
            _device: &DeviceId,
            _config: &StreamConfig,
            _callback: CallbackHandle,
        ) -> Result<Box<dyn AudioStream>> {
            Err(BackendError::Backend(
                "cpal backend not available in this build".into(),
            ))
        }
    }
}

pub type Result<T> = std::result::Result<T, BackendError>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamConfig {
    pub sample_rate: u32,
    pub channels: usize,
    pub block_size: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            block_size: 256,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceId(pub String);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub is_default_output: bool,
    pub is_default_input: bool,
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("stream configuration unsupported")]
    UnsupportedConfiguration,
    #[error("backend error: {0}")]
    Backend(String),
}

pub trait AudioStream: Send + Sync {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

pub trait AudioBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn devices(&self) -> Result<Vec<DeviceInfo>>;
    fn default_output(&self) -> Result<DeviceId>;
    fn open_output_stream(
        &self,
        device: &DeviceId,
        config: &StreamConfig,
        callback: CallbackHandle,
    ) -> Result<Box<dyn AudioStream>>;
}

#[cfg(feature = "cpal")]
pub(crate) fn render_callback(
    callback: &CallbackHandle,
    data: &mut [f32],
    channels: usize,
    sample_rate: u32,
) {
    let frames = data.len() / channels;
    let mut buffer = InterleavedAudioBuffer {
        inputs: &[],
        outputs: data,
        channels,
        frames,
        sample_rate,
        info: AudioCallbackInfo::default(),
    };
    callback.process(&mut buffer);
}
