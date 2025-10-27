#![cfg(all(feature = "openasio", target_os = "linux"))]

use harmoniq_engine::rt::backend::openasio::{harmoniq_asio_audio_cb, RtTrampoline};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

struct GuardAllocator;

static RT_ALLOC_GUARD: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for GuardAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if RT_ALLOC_GUARD.load(AtomicOrdering::Relaxed) {
            panic!("allocation while RT guard active");
        }
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if RT_ALLOC_GUARD.load(AtomicOrdering::Relaxed) {
            panic!("allocation while RT guard active");
        }
        System.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if RT_ALLOC_GUARD.load(AtomicOrdering::Relaxed) {
            panic!("reallocation while RT guard active");
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: GuardAllocator = GuardAllocator;

fn with_rt_alloc_guard<F: FnOnce()>(f: F) {
    RT_ALLOC_GUARD.store(true, AtomicOrdering::SeqCst);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    RT_ALLOC_GUARD.store(false, AtomicOrdering::SeqCst);
    result.expect("RT guard callback panicked");
}

extern "C" fn noop_rt_cb(
    _user: *mut core::ffi::c_void,
    _in_ptr: *const f32,
    _out_ptr: *mut f32,
    _frames: u32,
) {
}

#[test]
fn alloc_guard_rt_callback() {
    let mut tr = RtTrampoline::new(noop_rt_cb, core::ptr::null_mut(), 128, 0, 0);
    let in_arr: [*const f32; 1] = [core::ptr::null()];
    let mut out_arr: [*mut f32; 1] = [core::ptr::null_mut()];

    with_rt_alloc_guard(|| unsafe {
        harmoniq_asio_audio_cb(tr.user_token(), in_arr.as_ptr(), out_arr.as_mut_ptr(), 128);
    });

    assert_eq!(tr.seq.load(AtomicOrdering::Relaxed), 1);
    assert_eq!(tr.xruns.load(AtomicOrdering::Relaxed), 0);
    assert_eq!(tr.processed_frames.load(AtomicOrdering::Relaxed), 128);
}

#[test]
fn sustained_blocks_update_metrics() {
    let mut tr = RtTrampoline::new(noop_rt_cb, core::ptr::null_mut(), 64, 0, 0);
    let in_arr: [*const f32; 1] = [core::ptr::null()];
    let mut out_arr: [*mut f32; 1] = [core::ptr::null_mut()];

    for _ in 0..10_000u64 {
        unsafe {
            harmoniq_asio_audio_cb(tr.user_token(), in_arr.as_ptr(), out_arr.as_mut_ptr(), 64);
        }
    }

    assert_eq!(tr.seq.load(AtomicOrdering::Relaxed), 10_000);
    assert_eq!(
        tr.processed_frames.load(AtomicOrdering::Relaxed),
        10_000 * 64
    );
}

#[test]
#[ignore = "requires an OpenASIO driver build and hardware"]
fn hot_restart_cycle() {
    use harmoniq_engine::rt::backend::{AudioBackend, BackendKind, DeviceDesc};

    let mut backend = harmoniq_engine::rt::backend::make(BackendKind::OpenAsio);
    let desc = DeviceDesc {
        name: "default".into(),
        sr: 48_000,
        frames: 128,
        inputs: 0,
        outputs: 2,
    };

    backend
        .open(&desc, noop_rt_cb, core::ptr::null_mut())
        .expect("open backend");
    backend.start().expect("start backend");
    backend.stop().expect("stop backend");
    backend.close();
}
