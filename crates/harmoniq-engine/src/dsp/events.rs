use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::time::{LoopRegion, SharedTempoMap, TempoMap, Transport};

const STATE_PLAYING: u32 = 0b0001;
const STATE_LOOP_ENABLED: u32 = 0b0010;
const NO_EVENT: u32 = u32::MAX;

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
    tempo_map: ArcSwap<TempoMap>,
    state: AtomicU32,
    sample_pos: AtomicU64,
    pending_start: AtomicU32,
    pending_stop: AtomicU32,
    loop_start: AtomicU64,
    loop_end: AtomicU64,
    map_version: AtomicU64,
}

impl TransportClock {
    pub fn new() -> Self {
        Self::with_map(TempoMap::default())
    }

    pub fn with_map(map: TempoMap) -> Self {
        Self {
            inner: Arc::new(TransportAtomic {
                tempo_map: ArcSwap::new(Arc::new(map)),
                state: AtomicU32::new(0),
                sample_pos: AtomicU64::new(0),
                pending_start: AtomicU32::new(NO_EVENT),
                pending_stop: AtomicU32::new(NO_EVENT),
                loop_start: AtomicU64::new(0),
                loop_end: AtomicU64::new(0),
                map_version: AtomicU64::new(0),
            }),
        }
    }

    #[inline]
    pub fn load(&self) -> Transport {
        let sample_position = self.inner.sample_pos.load(Ordering::Relaxed);
        let state_bits = self.inner.state.load(Ordering::Relaxed);
        let is_playing = (state_bits & STATE_PLAYING) != 0;
        let map_version = self.inner.map_version.load(Ordering::Relaxed);
        let tempo_map = self.inner.tempo_map.load_full();
        let tempo = tempo_map.tempo_at(sample_position);
        let time_signature = tempo_map.time_signature_at(sample_position);
        Transport {
            tempo,
            time_signature,
            sample_position,
            is_playing,
            map_version,
            tempo_map,
        }
    }

    pub fn tempo_map(&self) -> SharedTempoMap {
        self.inner.tempo_map.load_full()
    }

    pub fn set_tempo_map(&self, map: TempoMap) {
        self.inner.tempo_map.store(Arc::new(map));
        self.inner.map_version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn seek(&self, sample_position: u64) {
        self.inner
            .sample_pos
            .store(sample_position, Ordering::Release);
    }

    pub fn start_immediately(&self) {
        self.inner.state.fetch_or(STATE_PLAYING, Ordering::AcqRel);
    }

    pub fn stop_immediately(&self) {
        self.inner.state.fetch_and(!STATE_PLAYING, Ordering::AcqRel);
    }

    pub fn schedule_start(&self, offset: u32) {
        self.inner.pending_start.store(offset, Ordering::Release);
    }

    pub fn schedule_stop(&self, offset: u32) {
        self.inner.pending_stop.store(offset, Ordering::Release);
    }

    pub fn set_loop_region(&self, region: Option<LoopRegion>) {
        match region {
            Some(region) if region.end > region.start => {
                self.inner.loop_start.store(region.start, Ordering::Release);
                self.inner.loop_end.store(region.end, Ordering::Release);
                self.inner
                    .state
                    .fetch_or(STATE_LOOP_ENABLED, Ordering::AcqRel);
            }
            _ => {
                self.inner.loop_end.store(0, Ordering::Release);
                self.inner.loop_start.store(0, Ordering::Release);
                self.inner
                    .state
                    .fetch_and(!STATE_LOOP_ENABLED, Ordering::AcqRel);
            }
        }
    }

    pub fn advance_samples(&self, frames: u32) {
        if frames == 0 {
            return;
        }

        let mut sample_pos = self.inner.sample_pos.load(Ordering::Relaxed);
        let mut state_bits = self.inner.state.load(Ordering::Relaxed);
        let mut playing = (state_bits & STATE_PLAYING) != 0;

        let start_raw = self.inner.pending_start.swap(NO_EVENT, Ordering::AcqRel);
        let stop_raw = self.inner.pending_stop.swap(NO_EVENT, Ordering::AcqRel);
        let mut start_offset = if start_raw == NO_EVENT {
            None
        } else {
            Some(start_raw as u64)
        };
        let mut stop_offset = if stop_raw == NO_EVENT {
            None
        } else {
            Some(stop_raw as u64)
        };

        let loop_start = self.inner.loop_start.load(Ordering::Relaxed);
        let loop_end = self.inner.loop_end.load(Ordering::Relaxed);
        let loop_enabled = (state_bits & STATE_LOOP_ENABLED) != 0 && loop_end > loop_start;

        let frames_u64 = frames as u64;
        for frame in 0..frames_u64 {
            if let Some(offset) = start_offset {
                if offset == frame {
                    playing = true;
                    state_bits |= STATE_PLAYING;
                    start_offset = None;
                }
            }

            if let Some(offset) = stop_offset {
                if offset == frame {
                    playing = false;
                    state_bits &= !STATE_PLAYING;
                    stop_offset = None;
                }
            }

            if playing {
                sample_pos = sample_pos.wrapping_add(1);
                if loop_enabled && sample_pos >= loop_end {
                    sample_pos = loop_start;
                }
            }
        }

        if let Some(offset) = start_offset {
            let remaining = offset.saturating_sub(frames_u64);
            self.inner
                .pending_start
                .store(remaining as u32, Ordering::Release);
        }

        if let Some(offset) = stop_offset {
            let remaining = offset.saturating_sub(frames_u64);
            self.inner
                .pending_stop
                .store(remaining as u32, Ordering::Release);
        }

        self.inner.sample_pos.store(sample_pos, Ordering::Release);
        self.inner.state.store(state_bits, Ordering::Release);
    }
}
