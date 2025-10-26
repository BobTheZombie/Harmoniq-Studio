use std::f32::consts::TAU;
use std::time::Instant;

use anyhow::Result;
use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::rt::{
    backend::{self, AudioBackend, BackendKind, DeviceDesc, RtCallback},
    metrics::{BlockStat, Metrics},
    thread,
};

pub struct Engine {
    backend: Box<dyn AudioBackend>,
    pub metrics: Metrics,
    pub sample_rate: u32,
    pub block: u32,
    expected_ns: u64,
    xruns: AtomicU32,
    inputs: u32,
    outputs: u32,
    pub user_cb: Option<RtCallback>,
    pub user_ctx: *mut c_void,
    tone_phase: f32,
    tone_freq: f32,
    tone_inc: f32,
    pub sched_engine: crate::engine::Engine,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            backend: backend::make(BackendKind::OpenAsio),
            metrics: Metrics::new(1024),
            sample_rate: 48_000,
            block: 128,
            expected_ns: 0,
            xruns: AtomicU32::new(0),
            inputs: 0,
            outputs: 2,
            user_cb: None,
            user_ctx: core::ptr::null_mut(),
            tone_phase: 0.0,
            tone_freq: 440.0,
            tone_inc: 0.0,
            sched_engine: crate::engine::Engine::new(48_000, 128, 1024),
        }
    }

    pub fn set_device(
        &mut self,
        kind: BackendKind,
        sr: u32,
        frames: u32,
        ins: u32,
        outs: u32,
    ) -> Result<()> {
        self.sample_rate = sr;
        self.block = frames;
        self.inputs = ins;
        self.outputs = outs;
        self.backend = backend::make(kind);
        self.xruns.store(0, Ordering::Relaxed);
        self.expected_ns = if sr > 0 {
            (frames as u64 * 1_000_000_000u64) / (sr as u64)
        } else {
            0
        };
        self.update_tone_increment();
        self.sched_engine.configure(sr, frames.max(1));
        self.sched_engine.reset();
        unsafe {
            thread::enter_hard_rt();
        }
        let ctx = self as *mut _ as *mut c_void;
        self.backend.open(
            &DeviceDesc {
                name: "default".into(),
                sr,
                frames,
                inputs: ins,
                outputs: outs,
            },
            Self::rt_trampoline,
            ctx,
        )
    }

    pub fn start(&mut self) -> Result<()> {
        self.backend.start()
    }

    pub fn stop(&mut self) -> Result<()> {
        self.backend.stop()
    }

    pub fn close(&mut self) {
        self.backend.close();
    }

    pub fn set_phase1_user_cb(&mut self, cb: RtCallback, ctx: *mut c_void) {
        self.user_cb = Some(cb);
        self.user_ctx = ctx;
    }

    pub fn enable_test_tone(&mut self, freq: f32) {
        self.tone_freq = freq.max(1.0);
        self.update_tone_increment();
        self.user_cb = Some(Self::test_tone_cb);
        self.user_ctx = self as *mut _ as *mut c_void;
    }

    fn update_tone_increment(&mut self) {
        if self.sample_rate > 0 {
            self.tone_inc = TAU * self.tone_freq / self.sample_rate as f32;
        } else {
            self.tone_inc = 0.0;
        }
    }

    extern "C" fn test_tone_cb(
        user: *mut c_void,
        _in_ptr: *const f32,
        out_ptr: *mut f32,
        frames: u32,
    ) {
        if user.is_null() || out_ptr.is_null() {
            return;
        }
        let engine = unsafe { &mut *(user as *mut Engine) };
        if engine.outputs == 0 {
            return;
        }
        let channels = engine.outputs as usize;
        let len = channels * frames as usize;
        let buffer = unsafe { core::slice::from_raw_parts_mut(out_ptr, len) };
        let mut phase = engine.tone_phase;
        let inc = engine.tone_inc;
        let gain = 0.1f32;
        for frame in 0..frames as usize {
            let sample = (phase).sin() * gain;
            let base = frame * channels;
            for ch in 0..channels {
                buffer[base + ch] = sample;
            }
            phase += inc;
            if phase >= TAU {
                phase -= TAU;
            }
        }
        engine.tone_phase = phase;
    }

    extern "C" fn rt_trampoline(
        user: *mut c_void,
        in_ptr: *const f32,
        out_ptr: *mut f32,
        frames: u32,
    ) {
        let start = Instant::now();
        let engine = unsafe { &mut *(user as *mut Engine) };
        if let Some(cb) = engine.user_cb {
            cb(engine.user_ctx, in_ptr, out_ptr, frames);
        } else {
            unsafe {
                crate::sched::executor::process_block(
                    &mut engine.sched_engine as *mut crate::engine::Engine,
                    in_ptr,
                    out_ptr,
                    frames,
                );
            }
        }

        let elapsed = start.elapsed();
        let nanos = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        let mut xruns = engine.xruns.load(Ordering::Relaxed);
        if engine.expected_ns > 0 && nanos > engine.expected_ns {
            xruns = engine.xruns.fetch_add(1, Ordering::Relaxed) + 1;
        }
        engine.metrics.write(BlockStat {
            ns: nanos,
            frames,
            xruns,
        });
    }
}
