use super::{
    render_callback, AudioBackend, AudioStream, BackendError, DeviceId, DeviceInfo, Result,
    StreamConfig,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use engine_rt::CallbackHandle;
use std::sync::Arc;

pub struct CpalBackend {
    host: cpal::Host,
}

impl Default for CpalBackend {
    fn default() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }
}

impl CpalBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn resolve_device(&self, device: &DeviceId) -> Result<cpal::Device> {
        let devices = self
            .host
            .devices()
            .map_err(|err| BackendError::Backend(err.to_string()))?;
        for dev in devices {
            if let Ok(name) = dev.name() {
                if name == device.0 {
                    return Ok(dev);
                }
            }
        }
        Err(BackendError::DeviceNotFound(device.0.clone()))
    }
}

impl AudioBackend for CpalBackend {
    fn name(&self) -> &'static str {
        "cpal"
    }

    fn devices(&self) -> Result<Vec<DeviceInfo>> {
        let mut result = Vec::new();
        let default_output = self
            .host
            .default_output_device()
            .and_then(|device| device.name().ok())
            .unwrap_or_default();
        let default_input = self
            .host
            .default_input_device()
            .and_then(|device| device.name().ok())
            .unwrap_or_default();
        let devices = self
            .host
            .devices()
            .map_err(|err| BackendError::Backend(err.to_string()))?;
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            let info = DeviceInfo {
                id: DeviceId(name.clone()),
                name: name.clone(),
                is_default_output: name == default_output,
                is_default_input: name == default_input,
            };
            result.push(info);
        }
        Ok(result)
    }

    fn default_output(&self) -> Result<DeviceId> {
        self.host
            .default_output_device()
            .and_then(|device| device.name().ok())
            .map(DeviceId)
            .ok_or_else(|| BackendError::Backend("no default output device".into()))
    }

    fn open_output_stream(
        &self,
        device: &DeviceId,
        config: &StreamConfig,
        callback: CallbackHandle,
    ) -> Result<Box<dyn AudioStream>> {
        let device = if device.0.is_empty() {
            self.host
                .default_output_device()
                .ok_or_else(|| BackendError::Backend("no default output device".into()))?
        } else {
            self.resolve_device(device)?
        };

        let mut supported = device
            .supported_output_configs()
            .map_err(|err| BackendError::Backend(err.to_string()))?;
        let desired_channels = config.channels as u16;
        let desired_sample_rate = cpal::SampleRate(config.sample_rate);
        let mut selected = None;
        while let Some(range) = supported.next() {
            if range.channels() == desired_channels
                && range.min_sample_rate() <= desired_sample_rate
                && range.max_sample_rate() >= desired_sample_rate
            {
                selected = Some(range.with_sample_rate(desired_sample_rate));
                break;
            }
        }
        let config = selected
            .ok_or(BackendError::UnsupportedConfiguration)?
            .config();

        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;
        let callback_handle = callback.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    render_callback(&callback_handle, data, channels, sample_rate);
                },
                move |err| {
                    tracing::error!("cpal stream error: {err}");
                },
                None,
            )
            .map_err(|err| BackendError::Backend(err.to_string()))?;

        Ok(Box::new(CpalStream {
            stream: Arc::new(stream),
        }))
    }
}

struct CpalStream {
    stream: Arc<cpal::Stream>,
}

impl AudioStream for CpalStream {
    fn start(&self) -> Result<()> {
        self.stream
            .play()
            .map_err(|err| BackendError::Backend(err.to_string()))
    }

    fn stop(&self) -> Result<()> {
        self.stream
            .pause()
            .map_err(|err| BackendError::Backend(err.to_string()))
    }
}
