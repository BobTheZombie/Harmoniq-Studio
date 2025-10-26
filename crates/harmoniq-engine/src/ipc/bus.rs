use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

use crate::engine::TransportState;
use crate::rt::metrics::BlockStat;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TransportSnapshot {
    pub state: TransportState,
    pub position_samples: u64,
    pub sample_rate: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsSnapshot {
    pub last_block_ns: u64,
    pub xruns: u32,
}

#[allow(dead_code)]
pub struct EngineEventWriter {
    transport_state: Arc<AtomicU32>,
    position: Arc<AtomicU64>,
    sample_rate: Arc<AtomicU32>,
    metrics: Arc<AtomicU64>,
    xruns: Arc<AtomicU32>,
    meters_tx: HeapProducer<BlockStat>,
}

unsafe impl Send for EngineEventWriter {}
unsafe impl Sync for EngineEventWriter {}

impl fmt::Debug for EngineEventWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineEventWriter").finish_non_exhaustive()
    }
}

#[allow(dead_code)]
impl EngineEventWriter {
    pub fn update_transport(&self, state: TransportState, position_samples: u64) {
        self.transport_state
            .store(encode_transport(state), Ordering::Relaxed);
        self.position.store(position_samples, Ordering::Relaxed);
    }

    pub fn update_sample_rate(&self, sr: u32) {
        self.sample_rate.store(sr, Ordering::Relaxed);
    }

    pub fn push_block_stat(&mut self, stat: BlockStat) {
        let _ = self.meters_tx.push(stat);
        self.metrics.store(stat.ns, Ordering::Relaxed);
        self.xruns.store(stat.xruns, Ordering::Relaxed);
    }
}

#[allow(dead_code)]
pub struct EngineEventReader {
    transport_state: Arc<AtomicU32>,
    position: Arc<AtomicU64>,
    sample_rate: Arc<AtomicU32>,
    metrics: Arc<AtomicU64>,
    xruns: Arc<AtomicU32>,
    meters_rx: HeapConsumer<BlockStat>,
    pending_vsync: Arc<AtomicBool>,
}

unsafe impl Send for EngineEventReader {}
unsafe impl Sync for EngineEventReader {}

impl fmt::Debug for EngineEventReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineEventReader").finish_non_exhaustive()
    }
}

#[allow(dead_code)]
impl EngineEventReader {
    pub fn snapshot_transport(&self) -> TransportSnapshot {
        TransportSnapshot {
            state: decode_transport(self.transport_state.load(Ordering::Relaxed)),
            position_samples: self.position.load(Ordering::Relaxed),
            sample_rate: self.sample_rate.load(Ordering::Relaxed),
        }
    }

    pub fn snapshot_metrics(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            last_block_ns: self.metrics.load(Ordering::Relaxed),
            xruns: self.xruns.load(Ordering::Relaxed),
        }
    }

    pub fn drain_block_stats(&mut self, out: &mut Vec<BlockStat>) {
        while let Some(stat) = self.meters_rx.pop() {
            out.push(stat);
        }
    }

    pub fn consume_vsync(&self) -> bool {
        self.pending_vsync.swap(false, Ordering::AcqRel)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct EngineEventBus {
    transport_state: Arc<AtomicU32>,
    position: Arc<AtomicU64>,
    sample_rate: Arc<AtomicU32>,
    metrics: Arc<AtomicU64>,
    xruns: Arc<AtomicU32>,
    pending_vsync: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl EngineEventBus {
    pub fn new(capacity: usize) -> (Self, EngineEventWriter, EngineEventReader) {
        let transport_state = Arc::new(AtomicU32::new(encode_transport(TransportState::Stopped)));
        let position = Arc::new(AtomicU64::new(0));
        let sample_rate = Arc::new(AtomicU32::new(0));
        let metrics = Arc::new(AtomicU64::new(0));
        let xruns = Arc::new(AtomicU32::new(0));
        let pending_vsync = Arc::new(AtomicBool::new(false));
        let ring = HeapRb::new(capacity.max(32));
        let (tx, rx) = ring.split();
        (
            Self {
                transport_state: Arc::clone(&transport_state),
                position: Arc::clone(&position),
                sample_rate: Arc::clone(&sample_rate),
                metrics: Arc::clone(&metrics),
                xruns: Arc::clone(&xruns),
                pending_vsync: Arc::clone(&pending_vsync),
            },
            EngineEventWriter {
                transport_state: Arc::clone(&transport_state),
                position: Arc::clone(&position),
                sample_rate: Arc::clone(&sample_rate),
                metrics: Arc::clone(&metrics),
                xruns: Arc::clone(&xruns),
                meters_tx: tx,
            },
            EngineEventReader {
                transport_state,
                position,
                sample_rate,
                metrics,
                xruns,
                meters_rx: rx,
                pending_vsync,
            },
        )
    }

    pub fn signal_vsync(&self) {
        self.pending_vsync.store(true, Ordering::Release);
    }
}

fn encode_transport(state: TransportState) -> u32 {
    match state {
        TransportState::Stopped => 0,
        TransportState::Playing => 1,
        TransportState::Recording => 2,
    }
}

fn decode_transport(value: u32) -> TransportState {
    match value {
        1 => TransportState::Playing,
        2 => TransportState::Recording,
        _ => TransportState::Stopped,
    }
}
