use core::sync::atomic::{
    AtomicBool, AtomicU32,
    Ordering::{AcqRel, Acquire, Relaxed, Release},
};
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::sched::events;

use super::{buffer, graph};

pub mod test_metrics {
    use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

    pub static SLEEP_COUNT: AtomicU32 = AtomicU32::new(0);
    pub static YIELD_COUNT: AtomicU32 = AtomicU32::new(0);

    pub fn reset() {
        SLEEP_COUNT.store(0, Relaxed);
        YIELD_COUNT.store(0, Relaxed);
    }
}

#[derive(Clone, Copy, Default)]
struct Job {
    idx: u32,
}

struct RtPoolInner {
    jobs: UnsafeJobs,
    job_count: AtomicU32,
    next_job: AtomicU32,
    done: AtomicU32,
    has_work: AtomicBool,
    stop: AtomicBool,
}

struct UnsafeJobs {
    buf: UnsafeCell<Box<[Job]>>,
}

unsafe impl Send for UnsafeJobs {}
unsafe impl Sync for UnsafeJobs {}

impl UnsafeJobs {
    fn new(capacity: usize) -> Self {
        let buf = vec![Job::default(); capacity.max(1)].into_boxed_slice();
        Self {
            buf: std::cell::UnsafeCell::new(buf),
        }
    }

    unsafe fn write(&self, idx: usize, job: Job) {
        let slice = &mut *self.buf.get();
        if idx < slice.len() {
            slice[idx] = job;
        }
    }

    unsafe fn get(&self) -> *const Job {
        (*self.buf.get()).as_ptr()
    }
}

pub struct RtPool {
    inner: Arc<RtPoolInner>,
    workers: Vec<JoinHandle<()>>,
    capacity: usize,
}

unsafe impl Send for RtPool {}
unsafe impl Sync for RtPool {}

impl RtPool {
    pub fn new(max_jobs: usize, workers: usize, worker_cores: &[usize]) -> Self {
        use crate::rt::cpu;

        let inner = Arc::new(RtPoolInner {
            jobs: UnsafeJobs::new(max_jobs.max(1)),
            job_count: AtomicU32::new(0),
            next_job: AtomicU32::new(0),
            done: AtomicU32::new(0),
            has_work: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });

        let mut handles = Vec::new();
        for (idx, core) in worker_cores.iter().copied().enumerate().take(workers) {
            let inner_clone = Arc::clone(&inner);
            let handle = thread::Builder::new()
                .name(format!("hq-wkr-{idx}"))
                .spawn(move || {
                    cpu::pin_current_thread_to(core);
                    worker_loop(inner_clone);
                })
                .expect("failed to spawn worker thread");
            handles.push(handle);
        }

        Self {
            capacity: max_jobs.max(1),
            inner,
            workers: handles,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn stage(&self, slot: usize, topo_idx: u32) {
        unsafe {
            self.inner.jobs.write(slot, Job { idx: topo_idx });
        }
    }

    pub fn submit(&self, job_count: usize) {
        if job_count == 0 {
            self.inner.job_count.store(0, Release);
            self.inner.next_job.store(0, Release);
            self.inner.done.store(0, Release);
            self.inner.has_work.store(false, Release);
            return;
        }

        let count = job_count.min(self.capacity) as u32;
        self.inner.job_count.store(count, Release);
        self.inner.next_job.store(0, Release);
        self.inner.done.store(0, Release);
        self.inner.has_work.store(true, Release);
    }

    pub fn wait(&self) {
        loop {
            let done = self.inner.done.load(Acquire);
            let total = self.inner.job_count.load(Acquire);
            if done >= total {
                break;
            }
            core::hint::spin_loop();
        }
        self.inner.has_work.store(false, Release);
    }

    pub fn jobs_ptr(&self) -> *const Job {
        unsafe { self.inner.jobs.get() }
    }
}

impl Drop for RtPool {
    fn drop(&mut self) {
        self.inner.stop.store(true, Release);
        self.inner.has_work.store(true, Release);
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop(pool: Arc<RtPoolInner>) {
    let mut idle_spins = 0u32;
    loop {
        if pool.stop.load(Acquire) {
            break;
        }
        if !pool.has_work.load(Acquire) {
            if idle_spins < 500 {
                core::hint::spin_loop();
                idle_spins = idle_spins.saturating_add(1);
                continue;
            }
            if idle_spins < 2000 {
                test_metrics::YIELD_COUNT.fetch_add(1, Relaxed);
                thread::yield_now();
                idle_spins = idle_spins.saturating_add(1);
                continue;
            }
            thread::sleep(Duration::from_micros(200));
            test_metrics::SLEEP_COUNT.fetch_add(1, Relaxed);
            idle_spins = 0;
            continue;
        }
        idle_spins = 0;
        let total = pool.job_count.load(Acquire);
        loop {
            let next = pool.next_job.fetch_add(1, AcqRel);
            if next >= total {
                break;
            }
            unsafe {
                crate::sched::executor::WORK::run_job(next as usize);
            }
            pool.done.fetch_add(1, AcqRel);
        }
    }
}

pub struct ExecShared<'a> {
    pub graph: &'a mut graph::Graph,
    pub bufs: &'a mut buffer::BlockBuffers,
    pub ev: events::EventSlice<'a>,
}

pub fn run_node(idx_topo: usize, shared: &mut ExecShared<'_>) {
    let node_id = shared.graph.topo[idx_topo] as usize;
    if let Some(node) = shared.graph.nodes.get_mut(node_id) {
        node.process(shared.bufs.as_audio(), &shared.ev);
    }
}

pub mod WORK {
    use super::{ExecShared, Job};
    use core::ptr;

    static mut SHARED: *mut ExecShared<'static> = ptr::null_mut();
    static mut JOBS: *const Job = ptr::null();

    pub unsafe fn set(shared: *mut ExecShared<'static>, jobs: *const Job) {
        SHARED = shared;
        JOBS = jobs;
    }

    pub unsafe fn run_job(idx: usize) {
        if SHARED.is_null() || JOBS.is_null() {
            return;
        }
        let shared = &mut *SHARED;
        let job = *JOBS.add(idx);
        super::run_node(job.idx as usize, shared);
    }
}

pub unsafe fn process_block(
    engine: *mut crate::engine::Engine,
    in_ptr: *const f32,
    out_ptr: *mut f32,
    frames: u32,
) {
    if engine.is_null() || frames == 0 {
        return;
    }

    let engine = &mut *engine;
    let mut bufs = unsafe { buffer::make_block(in_ptr, out_ptr, frames) };
    let events = events::slice_for_block(&engine.event_lane, engine.sample_pos, frames);
    let mut shared = ExecShared {
        graph: &mut engine.graph,
        bufs: &mut bufs,
        ev: events,
    };

    let depths = shared.graph.depths.clone();
    let use_pool = !engine.pool_disabled && engine.transport.playing.load(Acquire);

    for (start, end) in depths {
        if !use_pool {
            for idx in start..end {
                run_node(idx, &mut shared);
            }
            continue;
        }

        let mut staged = 0usize;
        for idx in start..end {
            let topo_idx = shared.graph.topo[idx];
            let node_idx = topo_idx as usize;
            if shared
                .graph
                .parallel_safe
                .get(node_idx)
                .copied()
                .unwrap_or(false)
            {
                if staged < engine.pool.capacity() {
                    engine.pool.stage(staged, idx as u32);
                    staged += 1;
                } else {
                    run_node(idx, &mut shared);
                }
            } else {
                run_node(idx, &mut shared);
            }
        }

        if staged > 0 {
            let shared_ptr = core::mem::transmute::<*mut ExecShared<'_>, *mut ExecShared<'static>>(
                &mut shared as *mut _,
            );
            WORK::set(shared_ptr, engine.pool.jobs_ptr());
            engine.pool.submit(staged);
            engine.pool.wait();
        }
    }

    engine.sample_pos = engine.sample_pos.saturating_add(frames as u64);
    engine
        .transport
        .sample_pos
        .store(engine.sample_pos, core::sync::atomic::Ordering::Relaxed);
}
