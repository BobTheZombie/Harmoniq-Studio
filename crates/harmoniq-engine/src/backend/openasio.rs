#![cfg(feature = "openasio")]

use anyhow::{anyhow, Result};
use openasio::{Driver as OaDriver, HostProcess, StreamConfig as OaCfg};
use std::ffi::c_void;

use crate::{
    backend::{AudioBackend, EngineRt, StreamConfig},
    buffers,
};

pub struct OpenAsioBackend {
    driver_path: String,
    device_name: Option<String>,
    desired: StreamConfig,
    driver: Option<OaDriver>,
}

impl OpenAsioBackend {
    pub fn new(driver_path: String, device_name: Option<String>, desired: StreamConfig) -> Self {
        Self {
            driver_path,
            device_name,
            desired,
            driver: None,
        }
    }
}

struct RtThunk {
    inner: Box<dyn EngineRt>,
    interleaved: bool,
    in_ch: usize,
    out_ch: usize,
}

impl HostProcess for RtThunk {
    #[inline]
    fn process(
        &mut self,
        in_ptr: *const c_void,
        out_ptr: *mut c_void,
        frames: u32,
        _cfg: &openasio::StreamConfig,
    ) -> bool {
        let nframes = frames as usize;
        if self.interleaved {
            let in_slice = if self.in_ch > 0 && !in_ptr.is_null() {
                unsafe { std::slice::from_raw_parts(in_ptr as *const f32, self.in_ch * nframes) }
            } else {
                &[]
            };
            let out_slice = if self.out_ch > 0 && !out_ptr.is_null() {
                unsafe {
                    std::slice::from_raw_parts_mut(out_ptr as *mut f32, self.out_ch * nframes)
                }
            } else {
                &mut []
            };
            let inputs = if self.in_ch > 0 {
                buffers::AudioView::from_interleaved_view(in_slice, self.in_ch, nframes)
            } else {
                buffers::AudioView::empty()
            };
            let mut outputs = if self.out_ch > 0 {
                buffers::AudioViewMut::from_interleaved_view(out_slice, self.out_ch, nframes)
            } else {
                buffers::AudioViewMut::empty()
            };
            self.inner.process(inputs, outputs, frames)
        } else {
            let in_planes = if self.in_ch > 0 && !in_ptr.is_null() {
                unsafe { std::slice::from_raw_parts(in_ptr as *const *const f32, self.in_ch) }
            } else {
                &[]
            };
            let out_planes = if self.out_ch > 0 && !out_ptr.is_null() {
                unsafe { std::slice::from_raw_parts_mut(out_ptr as *mut *mut f32, self.out_ch) }
            } else {
                &mut []
            };
            let inputs = if self.in_ch > 0 {
                buffers::AudioView::from_planes(in_planes, nframes)
            } else {
                buffers::AudioView::empty()
            };
            let mut outputs = if self.out_ch > 0 {
                buffers::AudioViewMut::from_planes(out_planes, nframes)
            } else {
                buffers::AudioViewMut::empty()
            };
            self.inner.process(inputs, outputs, frames)
        }
    }
}

impl AudioBackend for OpenAsioBackend {
    fn start(&mut self, rt: Box<dyn EngineRt>) -> Result<()> {
        let interleaved = self.desired.interleaved;
        let oa_cfg = OaCfg {
            sample_rate: self.desired.sample_rate,
            buffer_frames: self.desired.buffer_frames,
            in_channels: self.desired.in_channels,
            out_channels: self.desired.out_channels,
            interleaved,
        };
        let thunk = RtThunk {
            inner: rt,
            interleaved,
            in_ch: self.desired.in_channels as usize,
            out_ch: self.desired.out_channels as usize,
        };
        let mut drv = OaDriver::load(&self.driver_path, Box::new(thunk), oa_cfg, interleaved)
            .map_err(|e| anyhow!("OpenASIO load: {e}"))?;
        if let Some(name) = self.device_name.as_deref() {
            drv.open_by_name(Some(name))?;
        } else {
            drv.open_default()?;
        }
        let _caps = drv.caps();
        drv.start()?;
        self.driver = Some(drv);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut driver) = self.driver.take() {
            driver.stop();
        }
    }
}
