use core::ffi::c_void;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct DeviceDesc {
    pub name: String,
    pub sr: u32,
    pub frames: u32,
    pub inputs: u32,
    pub outputs: u32,
}

pub type RtCallback =
    extern "C" fn(user: *mut c_void, in_ptr: *const f32, out_ptr: *mut f32, frames: u32);

pub trait AudioBackend: Send {
    fn open(&mut self, desc: &DeviceDesc, cb: RtCallback, user: *mut c_void) -> Result<()>;
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn close(&mut self);
}

#[derive(Debug, Clone, Copy)]
pub enum BackendKind {
    OpenAsio,
    Alsa,
    Jack,
}

pub fn make(kind: BackendKind) -> Box<dyn AudioBackend> {
    match kind {
        BackendKind::OpenAsio => {
            #[cfg(feature = "openasio")]
            {
                Box::new(openasio::OpenAsioBackend::new())
            }
            #[cfg(not(feature = "openasio"))]
            {
                Box::new(StubBackend::new(
                    "OpenASIO backend requires the `openasio` feature",
                ))
            }
        }
        BackendKind::Alsa => Box::new(StubBackend::new("ALSA backend not implemented")),
        BackendKind::Jack => Box::new(StubBackend::new("JACK backend not implemented")),
    }
}

struct StubBackend {
    reason: &'static str,
}

impl StubBackend {
    const fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}

impl AudioBackend for StubBackend {
    fn open(&mut self, _desc: &DeviceDesc, _cb: RtCallback, _user: *mut c_void) -> Result<()> {
        Err(anyhow!(self.reason))
    }

    fn start(&mut self) -> Result<()> {
        Err(anyhow!(self.reason))
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn close(&mut self) {}
}

#[cfg(feature = "openasio")]
mod openasio {
    use super::{AudioBackend, DeviceDesc, RtCallback};
    use anyhow::{anyhow, Result};
    use openasio::{Driver as OaDriver, HostProcess, StreamConfig as OaCfg};
    use std::env;
    use std::ffi::c_void;

    const DEFAULT_DEBUG_DRIVER: &str = "target/debug/libopenasio_driver_cpal.so";
    const DEFAULT_RELEASE_DRIVER: &str = "target/release/libopenasio_driver_cpal.so";

    pub struct OpenAsioBackend {
        driver: Option<OaDriver>,
    }

    impl OpenAsioBackend {
        pub fn new() -> Self {
            Self { driver: None }
        }

        fn find_driver_path(desc: &DeviceDesc) -> Result<String> {
            if desc.name.ends_with(".so") {
                return Ok(desc.name.clone());
            }
            if let Ok(path) = env::var("HARMONIQ_OPENASIO_DRIVER") {
                return Ok(path);
            }
            if let Ok(path) = env::var("OPENASIO_DRIVER") {
                return Ok(path);
            }
            if let Ok(path) = env::var("OPENASIO_TEST_DRIVER") {
                return Ok(path);
            }
            let fallback = if cfg!(debug_assertions) {
                DEFAULT_DEBUG_DRIVER
            } else {
                DEFAULT_RELEASE_DRIVER
            };
            Ok(fallback.to_string())
        }
    }

    struct HostAdapter {
        cb: RtCallback,
        user: *mut c_void,
        channels_out: u32,
        channels_in: u32,
    }

    impl HostProcess for HostAdapter {
        #[inline]
        fn process(
            &mut self,
            in_ptr: *const c_void,
            out_ptr: *mut c_void,
            frames: u32,
            _cfg: &OaCfg,
        ) -> bool {
            let in_samples = if self.channels_in > 0 {
                in_ptr as *const f32
            } else {
                core::ptr::null()
            };
            let out_samples = if self.channels_out > 0 {
                out_ptr as *mut f32
            } else {
                core::ptr::null_mut()
            };
            (self.cb)(self.user, in_samples, out_samples, frames);
            true
        }
    }

    impl AudioBackend for OpenAsioBackend {
        fn open(&mut self, desc: &DeviceDesc, cb: RtCallback, user: *mut c_void) -> Result<()> {
            let driver_path = Self::find_driver_path(desc)?;
            let cfg = OaCfg {
                sample_rate: desc.sr,
                buffer_frames: desc.frames,
                in_channels: desc.inputs as u16,
                out_channels: desc.outputs as u16,
                interleaved: true,
            };
            let host = HostAdapter {
                cb,
                user,
                channels_out: desc.outputs,
                channels_in: desc.inputs,
            };
            let mut driver = OaDriver::load(&driver_path, Box::new(host), cfg, true)
                .map_err(|err| anyhow!("failed to load OpenASIO driver: {err:#}"))?;
            if desc.name != "default" {
                driver.open_by_name(Some(&desc.name))?;
            } else {
                driver.open_default()?;
            }
            self.driver = Some(driver);
            Ok(())
        }

        fn start(&mut self) -> Result<()> {
            if let Some(driver) = self.driver.as_mut() {
                driver.start()?;
                Ok(())
            } else {
                Err(anyhow!("OpenASIO backend not opened"))
            }
        }

        fn stop(&mut self) -> Result<()> {
            if let Some(driver) = self.driver.as_mut() {
                driver.stop();
            }
            Ok(())
        }

        fn close(&mut self) {
            self.driver = None;
        }
    }
}
