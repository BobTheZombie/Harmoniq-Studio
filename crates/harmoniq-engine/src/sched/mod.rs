pub mod buffer;
pub mod events;
pub mod executor;
pub mod graph;
pub mod pdc;

use buffer::AudioBuffers;
use events::{Ev, EventSlice};
use graph::{Node, NodeId, NodeMeta};

pub struct PassThrough {
    meta: NodeMeta,
}

impl PassThrough {
    pub fn new(id: NodeId, name: &'static str) -> Self {
        Self {
            meta: NodeMeta {
                id,
                name,
                latency: 0,
                tail: 0,
                parallel_safe: false,
            },
        }
    }
}

impl Node for PassThrough {
    fn meta(&self) -> &NodeMeta {
        &self.meta
    }

    fn prepare(&mut self, _sr: u32, _max_block: u32) {}

    fn process(&mut self, bufs: &mut AudioBuffers, _ev: &EventSlice) {
        let frames = bufs.nframes as usize;
        let left_in = bufs.ins[0];
        let right_in = bufs.ins[1];
        if bufs.outs[0].len() < frames || bufs.outs[1].len() < frames {
            return;
        }

        {
            let left_out = &mut bufs.outs[0];
            left_out
                .iter_mut()
                .take(frames)
                .for_each(|sample| *sample = 0.0);
            for (dst, src) in left_out.iter_mut().zip(left_in.iter()).take(frames) {
                *dst = *src;
            }
        }
        {
            let right_out = &mut bufs.outs[1];
            right_out
                .iter_mut()
                .take(frames)
                .for_each(|sample| *sample = 0.0);
            for (dst, src) in right_out.iter_mut().zip(right_in.iter()).take(frames) {
                *dst = *src;
            }
        }
    }
}

pub struct Gain {
    meta: NodeMeta,
    current: f32,
    param_id: u32,
}

impl Gain {
    pub fn new(id: NodeId, param_id: u32) -> Self {
        Self {
            meta: NodeMeta {
                id,
                name: "gain",
                latency: 0,
                tail: 0,
                parallel_safe: false,
            },
            current: 1.0,
            param_id,
        }
    }

    fn apply_gain(buffer: &mut AudioBuffers, start: usize, end: usize, gain: f32) {
        if start >= end {
            return;
        }
        let end = end.min(buffer.nframes as usize);
        if buffer.outs[0].len() < end || buffer.outs[1].len() < end {
            return;
        }
        {
            let left = &mut buffer.outs[0];
            for frame in start..end {
                left[frame] *= gain;
            }
        }
        {
            let right = &mut buffer.outs[1];
            for frame in start..end {
                right[frame] *= gain;
            }
        }
    }
}

impl Node for Gain {
    fn meta(&self) -> &NodeMeta {
        &self.meta
    }

    fn prepare(&mut self, _sr: u32, _max_block: u32) {
        self.current = 1.0;
    }

    fn process(&mut self, bufs: &mut AudioBuffers, ev: &EventSlice) {
        let mut cursor = 0usize;
        let mut gain = self.current;
        let frames = bufs.nframes as usize;

        for event in ev.ev.iter() {
            match *event {
                Ev::Param { id, norm, sample } if id == self.param_id => {
                    let target = (sample as usize).min(frames);
                    if target > cursor {
                        Self::apply_gain(bufs, cursor, target, gain);
                    }
                    gain = norm;
                    cursor = target;
                }
                _ => {}
            }
        }

        if cursor < frames {
            Self::apply_gain(bufs, cursor, frames, gain);
        }

        self.current = gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::sched::events::Ev;
    use crate::sched::executor;

    const SR: u32 = 48_000;
    const BLOCK: u32 = 128;

    fn make_engine() -> Engine {
        Engine::new(SR, BLOCK, 1024)
    }

    #[test]
    fn no_alloc_during_process() {
        let mut engine = make_engine();
        let frames = BLOCK;
        let total_blocks = (SR as u64 * 60) / frames as u64;
        let mut input = vec![0.0f32; (frames as usize) * 2];
        let mut output = vec![0.0f32; (frames as usize) * 2];

        crate::scratch::reset_allocation_count();
        let before = crate::scratch::allocation_count();
        unsafe {
            for _ in 0..total_blocks {
                executor::process_block(
                    &mut engine as *mut Engine,
                    input.as_ptr(),
                    output.as_mut_ptr(),
                    frames,
                );
            }
        }
        let after = crate::scratch::allocation_count();
        assert_eq!(before, after, "process_block allocated heap memory");
    }

    #[test]
    fn param_events_are_sample_accurate() {
        let mut engine = make_engine();
        let frames = BLOCK;
        let mut input = vec![1.0f32; (frames as usize) * 2];
        let mut output = vec![0.0f32; (frames as usize) * 2];

        engine
            .event_lane
            .push(Ev::Param {
                id: 0,
                norm: 0.0,
                sample: 0,
            })
            .unwrap();

        let target_sample = 32u32;
        engine
            .event_lane
            .push(Ev::Param {
                id: 0,
                norm: 1.0,
                sample: target_sample,
            })
            .unwrap();

        unsafe {
            executor::process_block(
                &mut engine as *mut Engine,
                input.as_ptr(),
                output.as_mut_ptr(),
                frames,
            );
        }

        let (left, right) = output.split_at(frames as usize);
        assert!(left[..target_sample as usize]
            .iter()
            .all(|sample| sample.abs() < f32::EPSILON));
        assert!(left[target_sample as usize..]
            .iter()
            .all(|sample| (*sample - 1.0).abs() < 1e-6));
        assert!(right[..target_sample as usize]
            .iter()
            .all(|sample| sample.abs() < f32::EPSILON));
        assert!(right[target_sample as usize..]
            .iter()
            .all(|sample| (*sample - 1.0).abs() < 1e-6));
    }
}
