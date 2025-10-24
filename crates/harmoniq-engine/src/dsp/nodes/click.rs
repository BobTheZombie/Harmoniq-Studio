use harmoniq_dsp::AudioBlockMut;

use crate::dsp::graph::{DspNode, ProcessContext};
use crate::time::{BeatInfo, Transport};

pub struct MetronomeClickNode {
    accent_gain: f32,
    beat_gain: f32,
    sample_rate: f32,
    next_beat: Option<BeatInfo>,
    last_map_version: u64,
    last_position: u64,
}

impl MetronomeClickNode {
    pub fn new(accent_gain: f32, beat_gain: f32) -> Self {
        Self {
            accent_gain,
            beat_gain,
            sample_rate: 48_000.0,
            next_beat: None,
            last_map_version: 0,
            last_position: 0,
        }
    }

    fn reset_state(&mut self) {
        self.next_beat = None;
        self.last_position = 0;
        self.last_map_version = 0;
    }

    fn ensure_next_beat(&mut self, transport: &Transport, sample_rate: f32) {
        if self.next_beat.is_none() {
            self.next_beat = transport
                .tempo_map
                .first_beat_at_or_after(sample_rate, transport.sample_position);
        }
    }

    fn clear_outputs(outputs: &mut AudioBlockMut<'_>) {
        let channels = outputs.channels() as usize;
        let frames = outputs.frames() as usize;
        for channel in 0..channels {
            let mut chan = unsafe { outputs.chan_mut(channel) };
            if let Some(slice) = unsafe { chan.as_mut_slice() } {
                slice.fill(0.0);
            } else {
                for frame in 0..frames {
                    unsafe { chan.write(frame, 0.0) };
                }
            }
        }
    }

    fn write_impulse(outputs: &mut AudioBlockMut<'_>, frame: usize, gain: f32) {
        let channels = outputs.channels() as usize;
        for channel in 0..channels {
            let mut chan = unsafe { outputs.chan_mut(channel) };
            unsafe { chan.write(frame, gain) };
        }
    }
}

impl Default for MetronomeClickNode {
    fn default() -> Self {
        Self::new(1.0, 0.4)
    }
}

impl DspNode for MetronomeClickNode {
    fn prepare(&mut self, sr: f32, _max_block: u32, _in_ch: u32, _out_ch: u32) {
        self.sample_rate = sr.max(1.0);
        self.reset_state();
    }

    fn reset(&mut self) {
        self.reset_state();
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        Self::clear_outputs(&mut ctx.outputs);
        if ctx.frames == 0 {
            return;
        }

        let transport = &ctx.transport;
        let block_start = transport.sample_position;
        let block_end = block_start + ctx.frames as u64;

        if transport.map_version != self.last_map_version || block_start < self.last_position {
            self.next_beat = transport
                .tempo_map
                .first_beat_at_or_after(self.sample_rate, block_start);
            self.last_map_version = transport.map_version;
        }

        if !transport.is_playing {
            self.ensure_next_beat(transport, self.sample_rate);
            self.last_position = block_end;
            return;
        }

        self.ensure_next_beat(transport, self.sample_rate);

        while let Some(current) = self.next_beat {
            if current.sample >= block_end {
                break;
            }
            if current.sample < block_start {
                self.next_beat = transport.tempo_map.beat_after(self.sample_rate, &current);
                continue;
            }

            let frame = (current.sample - block_start) as usize;
            let gain = if current.is_downbeat() {
                self.accent_gain
            } else {
                self.beat_gain
            };
            Self::write_impulse(&mut ctx.outputs, frame.min(ctx.frames as usize - 1), gain);
            self.next_beat = transport.tempo_map.beat_after(self.sample_rate, &current);
        }

        self.last_position = block_end;
    }
}
