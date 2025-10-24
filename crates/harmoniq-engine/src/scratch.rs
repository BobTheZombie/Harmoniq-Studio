use std::{marker::PhantomData, ptr};

#[cfg(deny_alloc_in_rt)]
use std::cell::Cell;

#[cfg(any(test, deny_alloc_in_rt))]
use std::alloc::{GlobalAlloc, Layout, System};

/// Scratch space reused across audio processing blocks. This allows temporary
/// buffers to be prepared ahead of time and reused without per-block
/// allocations.
#[derive(Default)]
pub struct Scratch {
    interleaved: Vec<f32>,
    planar_ptrs: Vec<*mut f32>,
}

impl Scratch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(interleaved_samples: usize, channels: usize) -> Self {
        let mut scratch = Self::new();
        scratch.ensure_interleaved(interleaved_samples);
        scratch.ensure_planar_ptrs(channels);
        scratch
    }

    pub fn ensure_interleaved(&mut self, samples: usize) -> &mut [f32] {
        if self.interleaved.len() < samples {
            self.interleaved.resize(samples, 0.0);
        }
        &mut self.interleaved[..samples]
    }

    pub fn interleaved_mut(&mut self, channels: usize, frames: usize) -> &mut [f32] {
        let required = channels.saturating_mul(frames);
        self.ensure_interleaved(required)
    }

    pub fn ensure_planar_ptrs(&mut self, channels: usize) -> &mut [*mut f32] {
        if self.planar_ptrs.len() < channels {
            self.planar_ptrs.resize(channels, ptr::null_mut());
        }
        &mut self.planar_ptrs[..channels]
    }
}

/// Guard that detects heap allocations while active. Used in tests to ensure
/// audio processing does not touch the allocator.
pub struct RtAllocGuard {
    _priv: PhantomData<()>,
}

impl RtAllocGuard {
    pub fn enter() -> Self {
        #[cfg(deny_alloc_in_rt)]
        ENTERED.with(|flag| {
            if flag.replace(true) {
                panic!("nested RtAllocGuard detected");
            }
        });
        Self { _priv: PhantomData }
    }
}

impl Drop for RtAllocGuard {
    fn drop(&mut self) {
        #[cfg(deny_alloc_in_rt)]
        ENTERED.with(|flag| {
            flag.set(false);
        });
    }
}

#[cfg(deny_alloc_in_rt)]
thread_local! {
    static ENTERED: Cell<bool> = const { Cell::new(false) };
}

#[cfg(deny_alloc_in_rt)]
#[inline(always)]
fn on_alloc() {
    ENTERED.with(|flag| {
        if flag.get() {
            panic!("heap allocation detected while processing audio");
        }
    });
}

#[cfg(any(test, deny_alloc_in_rt))]
pub struct GuardedAllocator;

#[cfg(any(test, deny_alloc_in_rt))]
unsafe impl GlobalAlloc for GuardedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(deny_alloc_in_rt)]
        on_alloc();
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        #[cfg(deny_alloc_in_rt)]
        on_alloc();
        System.realloc(ptr, layout, new_size)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        #[cfg(deny_alloc_in_rt)]
        on_alloc();
        System.alloc_zeroed(layout)
    }
}

#[cfg(any(test, deny_alloc_in_rt))]
#[global_allocator]
static GLOBAL_ALLOCATOR: GuardedAllocator = GuardedAllocator;
