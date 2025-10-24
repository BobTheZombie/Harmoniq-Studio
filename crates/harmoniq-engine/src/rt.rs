use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_queue::ArrayQueue;

/// Enables flush-to-zero and denormals-are-zero on supported CPUs.
#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn enable_ftz_daz() {
    unsafe {
        #[cfg(target_arch = "x86")]
        use std::arch::x86::{_mm_getcsr, _mm_setcsr};
        #[cfg(target_arch = "x86_64")]
        use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};

        const FTZ: u32 = 1 << 15;
        const DAZ: u32 = 1 << 6;
        let csr = _mm_getcsr();
        _mm_setcsr(csr | FTZ | DAZ);
    }
}

/// No-op implementation for non x86/x86_64 targets.
#[inline]
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn enable_ftz_daz() {}

/// Pins the current process's address space into RAM to avoid major page faults
/// during realtime processing. On platforms where this is not supported the
/// call becomes a no-op.
#[cfg(target_os = "linux")]
pub fn mlock_process() -> std::io::Result<()> {
    unsafe {
        let flags = libc::MCL_CURRENT | libc::MCL_FUTURE;
        if libc::mlockall(flags) != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EPERM) {
                // Insufficient permissions. Running without locked memory is
                // still acceptable so treat this as success.
                return Ok(());
            }
            return Err(err);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn mlock_process() -> std::io::Result<()> {
    Ok(())
}

/// Attempts to pin the current thread to the provided logical core. When
/// affinity management is not available, the call succeeds without making
/// changes.
#[cfg(all(target_os = "linux", feature = "core_affinity"))]
pub fn pin_current_thread(core: usize) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    let cores = core_affinity::get_core_ids()
        .ok_or_else(|| Error::new(ErrorKind::Other, "failed to query CPU topology"))?;
    if cores.is_empty() {
        return Err(Error::new(ErrorKind::NotFound, "no CPU cores reported"));
    }

    let target = cores
        .get(core)
        .cloned()
        .unwrap_or_else(|| cores[core % cores.len()].clone());

    if core_affinity::set_for_current(target) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            "failed to apply CPU affinity for realtime thread",
        ))
    }
}

#[cfg(not(all(target_os = "linux", feature = "core_affinity")))]
pub fn pin_current_thread(_core: usize) -> std::io::Result<()> {
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AudioMetrics {
    pub xruns: u64,
    pub last_block_ns: u64,
    pub max_block_ns: u64,
}

#[derive(Clone)]
pub struct AudioMetricsCollector {
    inner: Arc<AudioMetricsInner>,
}

impl AudioMetricsCollector {
    pub fn new(history_capacity: usize) -> Self {
        Self {
            inner: Arc::new(AudioMetricsInner {
                xruns: AtomicU64::new(0),
                last_block_ns: AtomicU64::new(0),
                max_block_ns: AtomicU64::new(0),
                history: MetricsRing::new(history_capacity),
            }),
        }
    }

    #[inline]
    pub fn snapshot(&self) -> AudioMetrics {
        AudioMetrics {
            xruns: self.inner.xruns.load(Ordering::Relaxed),
            last_block_ns: self.inner.last_block_ns.load(Ordering::Relaxed),
            max_block_ns: self.inner.max_block_ns.load(Ordering::Relaxed),
        }
    }

    #[inline]
    pub fn record_block(&self, duration: Duration, period_ns: u64) {
        let nanos = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.inner.last_block_ns.store(nanos, Ordering::Relaxed);
        let max_ns = self.inner.update_max(nanos);
        let xrun_count = if period_ns > 0 && nanos > period_ns {
            self.inner.xruns.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.inner.xruns.load(Ordering::Relaxed)
        };

        self.inner.push_history(AudioMetrics {
            xruns: xrun_count,
            last_block_ns: nanos,
            max_block_ns: max_ns.max(nanos),
        });
    }

    #[inline]
    pub fn register_xrun(&self) {
        let xruns = self.inner.xruns.fetch_add(1, Ordering::Relaxed) + 1;
        let last = self.inner.last_block_ns.load(Ordering::Relaxed);
        let max = self.inner.max_block_ns.load(Ordering::Relaxed);
        self.inner.push_history(AudioMetrics {
            xruns,
            last_block_ns: last,
            max_block_ns: max,
        });
    }

    pub fn drain_history(&self) -> Vec<AudioMetrics> {
        let mut metrics = Vec::new();
        while let Some(entry) = self.inner.history.pop() {
            metrics.push(entry);
        }
        metrics
    }

    pub fn reset(&self) {
        self.inner.xruns.store(0, Ordering::Relaxed);
        self.inner.last_block_ns.store(0, Ordering::Relaxed);
        self.inner.max_block_ns.store(0, Ordering::Relaxed);
        self.inner.history.clear();
    }
}

struct AudioMetricsInner {
    xruns: AtomicU64,
    last_block_ns: AtomicU64,
    max_block_ns: AtomicU64,
    history: MetricsRing,
}

impl AudioMetricsInner {
    #[inline]
    fn push_history(&self, metrics: AudioMetrics) {
        self.history.push(metrics);
    }

    #[inline]
    fn update_max(&self, candidate: u64) -> u64 {
        let mut current = self.max_block_ns.load(Ordering::Relaxed);
        while candidate > current {
            match self.max_block_ns.compare_exchange_weak(
                current,
                candidate,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return candidate,
                Err(previous) => current = previous,
            }
        }
        current
    }
}

/// Minimal lock-free ring buffer specialised for audio metrics snapshots.
///
/// The ring is single-producer multi-consumer: only the realtime audio thread
/// writes metrics, while any control thread may drain snapshots. Internally it
/// leverages [`ArrayQueue`], which performs all operations with atomics and no
/// dynamic allocations after construction.
struct MetricsRing {
    queue: ArrayQueue<AudioMetrics>,
}

impl MetricsRing {
    fn new(capacity: usize) -> Self {
        Self {
            queue: ArrayQueue::new(capacity.max(16)),
        }
    }

    #[inline]
    fn push(&self, metrics: AudioMetrics) {
        if self.queue.push(metrics).is_err() {
            let _ = self.queue.pop();
            let _ = self.queue.push(metrics);
        }
    }

    #[inline]
    fn pop(&self) -> Option<AudioMetrics> {
        self.queue.pop()
    }

    fn clear(&self) {
        while self.queue.pop().is_some() {}
    }
}
