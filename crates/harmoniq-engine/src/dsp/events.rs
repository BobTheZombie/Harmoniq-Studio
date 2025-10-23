use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default)]
pub struct Transport {
    pub tempo: f64,
    pub time_sig_num: u8,
    pub time_sig_den: u8,
    pub sample_pos: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MidiEvent {
    pub sample_offset: u32,
    pub data: [u8; 3],
    pub length: u8,
}

impl MidiEvent {
    #[inline]
    pub fn new(sample_offset: u32, data: [u8; 3]) -> Self {
        Self {
            sample_offset,
            data,
            length: 3,
        }
    }
}

#[derive(Clone)]
pub struct TransportClock {
    inner: Arc<TransportAtomic>,
}

struct TransportAtomic {
    tempo: AtomicU64,
    signature: AtomicU32,
    sample_pos: AtomicU64,
}

impl TransportClock {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TransportAtomic {
                tempo: AtomicU64::new((120.0f64).to_bits()),
                signature: AtomicU32::new((4u32 << 16) | 4u32),
                sample_pos: AtomicU64::new(0),
            }),
        }
    }

    #[inline]
    pub fn load(&self) -> Transport {
        let tempo_bits = self.inner.tempo.load(Ordering::Relaxed);
        let tempo = f64::from_bits(tempo_bits);
        let signature = self.inner.signature.load(Ordering::Relaxed);
        let num = (signature >> 16) as u8;
        let den = (signature & 0xFFFF) as u8;
        let pos = self.inner.sample_pos.load(Ordering::Relaxed);
        Transport {
            tempo,
            time_sig_num: num,
            time_sig_den: den,
            sample_pos: pos,
        }
    }

    #[inline]
    pub fn store(&self, transport: Transport) {
        self.inner
            .tempo
            .store(transport.tempo.to_bits(), Ordering::Relaxed);
        let packed = ((transport.time_sig_num as u32) << 16) | transport.time_sig_den as u32;
        self.inner.signature.store(packed, Ordering::Relaxed);
        self.inner
            .sample_pos
            .store(transport.sample_pos, Ordering::Relaxed);
    }

    #[inline]
    pub fn advance_samples(&self, samples: u32) {
        self.inner
            .sample_pos
            .fetch_add(samples as u64, Ordering::Relaxed);
    }
}
