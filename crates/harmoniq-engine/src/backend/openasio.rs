//! OpenASIO backend (Linux) – RT-safe adapter
//! - Planar outputs, double-buffer aware (buffer_index)
//! - No allocations/locks in the callback
//! - Silence on error paths, clamp/clean NaN/Inf
//! Enabled only when compiled on Linux with feature `openasio_gpl`.

#![cfg(all(target_os = "linux", feature = "openasio_gpl"))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use openasio_sdk::{
    host::{AsioDriver, BufferInfo, Callbacks, SampleType},
    registry::{enumerate_drivers, DriverId},
};

use super::safety::{deinterleave_f32_channel, enable_denormal_kill_once, sanitize_f32, zero_f32};

/// Thin wrapper that adapts OpenASIO to Harmoniq's interleaved render path.
pub struct OpenAsioBackend {
    driver: AsioDriver,
    frames: usize,
    channels_out: usize,
    /// Interleaved scratch buffer; allocated up-front.
    tmp_interleaved: Vec<f32>,
    running: Arc<AtomicBool>,
    buffers: Option<Vec<BufferInfo>>,
    sample_types: Vec<SampleType>,
    render: Option<Box<dyn FnMut(&mut [f32], usize, usize) + Send>>,
}

impl OpenAsioBackend {
    /// Enumerate available drivers: returns `(DriverId, driver_name)`.
    pub fn enumerate() -> Result<Vec<(DriverId, String)>> {
        let drivers = enumerate_drivers().context("enumerate OpenASIO drivers")?;
        Ok(drivers.into_iter().map(|d| (d.id, d.name)).collect())
    }

    /// Create a backend targeting the given driver and sample rate (Hz).
    pub fn new(driver_id: DriverId, sample_rate_hz: f64) -> Result<Self> {
        let mut driver = AsioDriver::open(driver_id).context("open OpenASIO driver")?;

        let (_min, _max, preferred, _granularity) = driver
            .get_buffer_size()
            .context("query OpenASIO buffer size")?;
        let frames = preferred.max(32) as usize;

        driver
            .set_sample_rate(sample_rate_hz)
            .context("set OpenASIO sample rate")?;

        let (_inputs, outputs) = driver.get_channels().context("query OpenASIO channels")?;
        if outputs <= 0 {
            bail!("ASIO driver reports zero output channels");
        }
        let channels_out = outputs as usize;

        let tmp_interleaved = vec![0.0f32; frames * channels_out];

        Ok(Self {
            driver,
            frames,
            channels_out,
            tmp_interleaved,
            running: Arc::new(AtomicBool::new(false)),
            buffers: None,
            sample_types: Vec::new(),
            render: None,
        })
    }

    /// Start the driver with the supplied RT-safe render callback.
    ///
    /// `render` receives an interleaved slice (frames * channels) that must be filled with
    /// samples in the range [-1.0, 1.0].
    pub fn start<F>(&mut self, render: F) -> Result<()>
    where
        F: FnMut(&mut [f32], usize, usize) + Send + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            bail!("OpenASIO backend already running");
        }

        enable_denormal_kill_once();

        self.render = Some(Box::new(render));
        let render_ptr = self
            .render
            .as_mut()
            .map(|f| &mut **f as *mut (dyn FnMut(&mut [f32], usize, usize) + Send))
            .ok_or_else(|| anyhow!("render callback missing"))?;

        // Prepare driver buffers.
        let buffers = self
            .driver
            .prepare_output_buffers(self.channels_out as i32, self.frames as i32)
            .context("prepare OpenASIO output buffers")?;
        let sample_types = self
            .driver
            .output_sample_types()
            .context("query OpenASIO output sample types")?;
        if sample_types.len() < self.channels_out {
            bail!("driver returned fewer sample types than output channels");
        }

        self.buffers = Some(buffers);
        self.sample_types = sample_types;

        let buffers_ptr = self
            .buffers
            .as_mut()
            .map(|b| b.as_mut_ptr())
            .ok_or_else(|| anyhow!("OpenASIO buffers missing"))?;
        let buffers_len = self.buffers.as_ref().map(|b| b.len()).unwrap_or(0);
        let sample_types_ptr = self.sample_types.as_ptr();
        let sample_types_len = self.sample_types.len();
        let tmp_ptr = self.tmp_interleaved.as_mut_ptr();
        let frames = self.frames;
        let channels = self.channels_out;
        let running_flag = self.running.clone();
        running_flag.store(true, Ordering::SeqCst);

        let callbacks = Callbacks {
            buffer_switch: move |buffer_index: i32| {
                if !running_flag.load(Ordering::Relaxed) {
                    return;
                }

                let buffer_index = buffer_index as usize;

                // Silence: zero all planes for the buffer we're about to touch.
                let buffers = unsafe { std::slice::from_raw_parts_mut(buffers_ptr, buffers_len) };
                let sample_types =
                    unsafe { std::slice::from_raw_parts(sample_types_ptr, sample_types_len) };

                for (ch, plane) in buffers.iter_mut().enumerate() {
                    match sample_types.get(ch).copied() {
                        Some(SampleType::Float32) => unsafe {
                            let out = plane.plane_mut_f32(buffer_index);
                            zero_f32(out);
                        },
                        Some(SampleType::Int32) => {
                            // TODO: implement f32 -> i32 path if a driver reports Int32.
                            // let out = unsafe { plane.plane_mut_i32(buffer_index) };
                            // for sample in out.iter_mut() { *sample = 0; }
                        }
                        _ => {}
                    }
                }

                if channels == 0 || frames == 0 {
                    return;
                }

                let out_interleaved =
                    unsafe { std::slice::from_raw_parts_mut(tmp_ptr, frames * channels) };

                unsafe {
                    (*render_ptr)(out_interleaved, frames, channels);
                }

                sanitize_f32(out_interleaved);

                for (ch, plane) in buffers.iter_mut().enumerate() {
                    match sample_types.get(ch).copied() {
                        Some(SampleType::Float32) => unsafe {
                            let out_plane = plane.plane_mut_f32(buffer_index);
                            deinterleave_f32_channel(
                                out_interleaved,
                                out_plane,
                                frames,
                                channels,
                                ch,
                            );
                        },
                        Some(SampleType::Int32) => {
                            // TODO: convert to i32 when needed.
                        }
                        _ => {}
                    }
                }
            },
            sample_rate_did_change: |_sr| {},
            asio_message: |_selector, _value, _msg, _opt| 0,
        };

        self.driver
            .start(&callbacks)
            .context("start OpenASIO driver")?;
        Ok(())
    }

    /// Stop streaming and release driver buffers.
    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::SeqCst) {
            let _ = self.driver.stop();
            let _ = self.driver.dispose_buffers();
        }
        self.buffers = None;
        self.sample_types.clear();
        let _ = self.render.take();
    }
}

impl Drop for OpenAsioBackend {
    fn drop(&mut self) {
        self.stop();
    }
}
