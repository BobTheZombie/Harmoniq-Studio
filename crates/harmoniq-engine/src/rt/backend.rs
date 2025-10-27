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
pub mod openasio {
    use super::{AudioBackend, DeviceDesc, RtCallback};
    use anyhow::{anyhow, Result};
    use core::sync::atomic::{AtomicU64, Ordering};
    use openasio::{Driver as OaDriver, HostProcess, StreamConfig as OaCfg};
    use std::env;
    use std::ffi::c_void;
    use std::ptr::NonNull;

    const DEFAULT_DEBUG_DRIVER: &str = "target/debug/libopenasio_driver_cpal.so";
    const DEFAULT_RELEASE_DRIVER: &str = "target/release/libopenasio_driver_cpal.so";

    #[repr(C)]
    pub struct RtTrampoline {
        pub engine_cb: RtCallback,
        pub user: *mut c_void,
        pub frames: u32,
        pub ins: u32,
        pub outs: u32,
        pub seq: AtomicU64,
        pub processed_frames: AtomicU64,
        pub xruns: AtomicU64,
    }

    impl RtTrampoline {
        pub fn new(cb: RtCallback, user: *mut c_void, frames: u32, ins: u32, outs: u32) -> Self {
            Self {
                engine_cb: cb,
                user,
                frames,
                ins,
                outs,
                seq: AtomicU64::new(0),
                processed_frames: AtomicU64::new(0),
                xruns: AtomicU64::new(0),
            }
        }

        #[inline]
        pub fn user_token(&self) -> *mut c_void {
            self as *const _ as *mut _
        }
    }

    #[no_mangle]
    pub extern "C" fn harmoniq_asio_audio_cb(
        user: *mut c_void,
        in_ptrs: *const *const f32,
        out_ptrs: *mut *mut f32,
        frames: u32,
    ) {
        // STRICT RT: no allocations, locks, syscalls, or logging here.
        let tr = unsafe { &mut *(user as *mut RtTrampoline) };
        debug_assert_eq!(frames, tr.frames, "frames mismatch");

        let in0 = if tr.ins > 0 && !in_ptrs.is_null() {
            unsafe { *in_ptrs }
        } else {
            core::ptr::null()
        };
        let out0 = if tr.outs > 0 && !out_ptrs.is_null() {
            unsafe { *out_ptrs }
        } else {
            core::ptr::null_mut()
        };

        (tr.engine_cb)(tr.user, in0, out0, frames);
        if frames != tr.frames {
            tr.xruns.fetch_add(1, Ordering::Release);
        }
        tr.seq.fetch_add(1, Ordering::Release);
        tr.processed_frames
            .fetch_add(frames as u64, Ordering::Release);
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct RtMetrics {
        pub seq: u64,
        pub frames: u64,
        pub xruns: u64,
    }

    pub struct OpenAsioBackend {
        driver: Option<OaDriver>,
        tr: Option<Box<RtTrampoline>>,
        sr: u32,
        frames: u32,
        ins: u32,
        outs: u32,
        caps: u32,
        opened: bool,
        running: bool,
    }

    impl OpenAsioBackend {
        pub fn new() -> Self {
            Self {
                driver: None,
                tr: None,
                sr: 0,
                frames: 0,
                ins: 0,
                outs: 0,
                opened: false,
                running: false,
                caps: 0,
            }
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

        pub fn metrics(&self) -> Option<RtMetrics> {
            self.tr.as_ref().map(|tr| RtMetrics {
                seq: tr.seq.load(Ordering::Acquire),
                frames: tr.processed_frames.load(Ordering::Acquire),
                xruns: tr.xruns.load(Ordering::Acquire),
            })
        }

        pub fn trampoline(&self) -> Option<&RtTrampoline> {
            self.tr.as_deref()
        }
    }

    struct HostAdapter {
        trampoline: NonNull<RtTrampoline>,
    }

    impl HostAdapter {
        fn new(trampoline: NonNull<RtTrampoline>) -> Self {
            Self { trampoline }
        }
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
            let in_planes = in_ptr as *const *const f32;
            let out_planes = out_ptr as *mut *mut f32;
            unsafe {
                harmoniq_asio_audio_cb(
                    self.trampoline.as_ptr() as *mut c_void,
                    in_planes,
                    out_planes,
                    frames,
                );
            }
            true
        }
    }

    impl AudioBackend for OpenAsioBackend {
        fn open(&mut self, desc: &DeviceDesc, cb: RtCallback, user: *mut c_void) -> Result<()> {
            if self.running {
                return Err(anyhow!("cannot open OpenASIO backend while running"));
            }
            if self.opened {
                self.close();
            }

            let driver_path = Self::find_driver_path(desc)?;
            let cfg = OaCfg {
                sample_rate: desc.sr,
                buffer_frames: desc.frames,
                in_channels: desc.inputs as u16,
                out_channels: desc.outputs as u16,
                interleaved: false,
            };

            let mut trampoline = Box::new(RtTrampoline::new(
                cb,
                user,
                cfg.buffer_frames,
                desc.inputs,
                desc.outputs,
            ));
            let trampoline_ptr = NonNull::from(trampoline.as_mut());
            let host = HostAdapter::new(trampoline_ptr);

            let mut driver = OaDriver::load(&driver_path, Box::new(host), cfg, false)
                .map_err(|err| anyhow!("failed to load OpenASIO driver: {err:#}"))?;
            if desc.name != "default" {
                driver.open_by_name(Some(&desc.name))?;
            } else {
                driver.open_default()?;
            }

            let mut negotiated = driver.default_config().unwrap_or(cfg);
            negotiated.interleaved = false;

            let frames = negotiated.buffer_frames.max(1);
            let ins = u32::from(negotiated.in_channels);
            let outs = u32::from(negotiated.out_channels);

            {
                let mut_ref = trampoline.as_mut();
                mut_ref.frames = frames;
                mut_ref.ins = ins;
                mut_ref.outs = outs;
                mut_ref.seq.store(0, Ordering::Relaxed);
                mut_ref.processed_frames.store(0, Ordering::Relaxed);
                mut_ref.xruns.store(0, Ordering::Relaxed);
            }

            self.sr = negotiated.sample_rate.max(1);
            self.frames = frames;
            self.ins = ins;
            self.outs = outs;
            self.caps = driver.caps();
            self.tr = Some(trampoline);
            self.driver = Some(driver);
            self.opened = true;
            self.running = false;
            Ok(())
        }

        fn start(&mut self) -> Result<()> {
            if let Some(driver) = self.driver.as_mut() {
                driver.start()?;
                self.running = true;
                Ok(())
            } else {
                Err(anyhow!("OpenASIO backend not opened"))
            }
        }

        fn stop(&mut self) -> Result<()> {
            if let Some(driver) = self.driver.as_mut() {
                driver.stop();
            }
            self.running = false;
            Ok(())
        }

        fn close(&mut self) {
            self.driver = None;
            self.tr = None;
            self.opened = false;
            self.running = false;
            self.caps = 0;
        }
    }
}
