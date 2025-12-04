use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::output::MidiOutputHandle;

/// Number of MIDI clock ticks per quarter note.
pub const MIDI_CLOCK_TICKS_PER_QUARTER: u32 = 24;

/// Utility for mapping monotonic timestamps into audio sample offsets.
#[derive(Debug, Clone)]
pub struct MidiClock {
    sample_rate: u32,
    last_nanos: u64,
    last_sample_pos: u64,
}

impl MidiClock {
    /// Create a new clock for the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            last_nanos: 0,
            last_sample_pos: 0,
        }
    }

    /// Update the clock with transport progress information.
    pub fn advance_transport(&mut self, nanos: u64, sample_pos: u64) {
        self.last_nanos = nanos;
        self.last_sample_pos = sample_pos;
    }

    /// Translate a monotonic timestamp into a sample offset within an audio block.
    pub fn to_block_sample(
        &mut self,
        now_nanos: u64,
        block_start_sample: u64,
        block_frames: u32,
    ) -> u32 {
        if block_frames == 0 {
            return 0;
        }
        let nanos_per_sample = 1_000_000_000u64 / self.sample_rate.max(1) as u64;
        let expected_sample = if now_nanos >= self.last_nanos {
            self.last_sample_pos + (now_nanos - self.last_nanos) / nanos_per_sample
        } else {
            block_start_sample
        };
        let mut offset = expected_sample.saturating_sub(block_start_sample) as i64;
        if offset < 0 {
            offset = 0;
        }
        if offset as u32 >= block_frames {
            block_frames - 1
        } else {
            offset as u32
        }
    }
}

/// Background MIDI clock generator that emits timing messages on a dedicated thread.
pub struct MidiClockSender {
    tempo_bpm: f32,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    output: Arc<Mutex<MidiOutputHandle>>,
}

impl MidiClockSender {
    /// Create a new clock sender bound to an output connection.
    pub fn new(tempo_bpm: f32, output: MidiOutputHandle) -> Self {
        Self {
            tempo_bpm,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            output: Arc::new(Mutex::new(output)),
        }
    }

    /// Update the tempo used to derive the tick interval.
    pub fn set_tempo(&mut self, tempo_bpm: f32) {
        self.tempo_bpm = tempo_bpm.max(1.0);
    }

    /// Start emitting timing clock messages and send a MIDI Start event.
    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        self.send_immediate([0xFA]);
        let running = Arc::clone(&self.running);
        let output = Arc::clone(&self.output);
        let interval = tick_interval(self.tempo_bpm);
        self.handle = Some(thread::spawn(move || {
            let mut next_tick = Instant::now();
            while running.load(Ordering::Acquire) {
                next_tick += interval;
                if let Ok(mut out) = output.lock() {
                    let _ = out.send(&[0xF8]);
                }
                let now = Instant::now();
                if next_tick > now {
                    thread::sleep(next_tick - now);
                } else {
                    next_tick = now;
                }
            }
        }));
    }

    /// Stop the clock thread and send a MIDI Stop event.
    pub fn stop(&mut self) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.send_immediate([0xFC]);
    }

    /// Resume the clock thread without resetting tick phase.
    pub fn continue_playback(&mut self) {
        if self.running.load(Ordering::Acquire) {
            return;
        }
        self.send_immediate([0xFB]);
        self.start();
    }

    fn send_immediate(&self, message: [u8; 1]) {
        if let Ok(mut out) = self.output.lock() {
            let _ = out.send(&message);
        }
    }
}

impl Drop for MidiClockSender {
    fn drop(&mut self) {
        if self.running.load(Ordering::Acquire) {
            self.stop();
        }
    }
}

/// Estimates tempo from incoming MIDI clock ticks.
pub struct MidiClockReceiver {
    history: VecDeque<Instant>,
    window: usize,
}

impl MidiClockReceiver {
    /// Create a new receiver that averages over the last `window` ticks.
    pub fn new(window: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(window.max(1)),
            window: window.max(1),
        }
    }

    /// Register a received timing clock and return the estimated tempo if available.
    pub fn register_tick(&mut self, now: Instant) -> Option<f32> {
        self.history.push_back(now);
        while self.history.len() > self.window {
            self.history.pop_front();
        }

        if self.history.len() < 2 {
            return None;
        }

        let mut iter = self.history.iter();
        let mut prev = *iter.next().unwrap();
        let mut total = Duration::from_secs(0);
        let mut count = 0u32;
        for &current in iter {
            total += current.saturating_duration_since(prev);
            prev = current;
            count += 1;
        }

        if count == 0 {
            return None;
        }

        let average_tick = total / count;
        if average_tick.is_zero() {
            return None;
        }

        let tick_secs = average_tick.as_secs_f32();
        let quarter_duration = tick_secs * MIDI_CLOCK_TICKS_PER_QUARTER as f32;
        Some(60.0 / quarter_duration)
    }

    /// Clear accumulated tick history, typically on MIDI Start.
    pub fn reset(&mut self) {
        self.history.clear();
    }
}

fn tick_interval(tempo_bpm: f32) -> Duration {
    let clamped = tempo_bpm.max(1.0);
    let nanos =
        (60_000_000_000f64 / (clamped as f64 * MIDI_CLOCK_TICKS_PER_QUARTER as f64)).max(1.0);
    Duration::from_nanos(nanos as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_nanos_to_block_offset() {
        let mut clock = MidiClock::new(48_000);
        clock.advance_transport(0, 1000);
        let offset = clock.to_block_sample(1_000_000, 1000, 128);
        assert_eq!(offset, 48); // 1ms at 48k = 48 samples

        let late = clock.to_block_sample(10_000_000_000, 1000, 128);
        assert_eq!(late, 127); // clamped to end of block
    }

    #[test]
    fn tick_interval_scales_with_tempo() {
        let fast = tick_interval(180.0);
        let slow = tick_interval(90.0);
        assert!(fast < slow);
    }

    #[test]
    fn receiver_estimates_tempo() {
        let mut recv = MidiClockReceiver::new(24);
        let start = Instant::now();
        let tick = Duration::from_millis(100);
        for i in 0..24 {
            let ts = start + tick * i;
            recv.register_tick(ts);
        }
        let bpm = recv.register_tick(start + tick * 24).unwrap();
        assert!((bpm - 25.0).abs() < 0.1);
    }
}
