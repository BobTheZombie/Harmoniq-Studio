use harmoniq_dsp::delay::StereoDelay;

use crate::dsp::graph::{DspNode, NodeLatency, ProcessContext};
use crate::dsp::params::ParamUpdate;

pub struct StereoDelayNode {
    delay: StereoDelay,
    time_seconds: f32,
    feedback: f32,
    mix: f32,
    sample_rate: f32,
    max_delay: f32,
}

impl StereoDelayNode {
    pub fn new(sample_rate: f32, max_delay: f32) -> Self {
        let mut delay = StereoDelay::new(sample_rate, max_delay);
        delay.set_time_seconds(0.25);
        delay.set_feedback(0.35);
        delay.set_mix(0.3);
        Self {
            delay,
            time_seconds: 0.25,
            feedback: 0.35,
            mix: 0.3,
            sample_rate: sample_rate.max(1.0),
            max_delay: max_delay.max(0.1),
        }
    }
}

impl DspNode for StereoDelayNode {
    fn prepare(&mut self, sr: f32, _max_block: u32, _in: u32, _out: u32) {
        self.sample_rate = sr.max(1.0);
        self.delay.prepare(self.sample_rate, self.max_delay);
        self.delay.set_time_seconds(self.time_seconds);
        self.delay.set_feedback(self.feedback);
        self.delay.set_mix(self.mix);
    }

    fn latency(&self) -> NodeLatency {
        NodeLatency {
            samples: (self.time_seconds * self.sample_rate) as u32,
        }
    }

    fn param(&mut self, update: ParamUpdate) {
        match update.id {
            0 => {
                self.time_seconds = update.value.clamp(0.0, self.max_delay);
                self.delay.set_time_seconds(self.time_seconds);
            }
            1 => {
                self.feedback = update.value;
                self.delay.set_feedback(self.feedback);
            }
            2 => {
                self.mix = update.value;
                self.delay.set_mix(self.mix);
            }
            _ => {}
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        let frames = ctx.frames as usize;
        let channels = ctx.outputs.channels().min(ctx.inputs.channels()) as usize;
        for frame in 0..frames {
            let input_l = unsafe { ctx.inputs.read_sample(0, frame) };
            let input_r = if channels > 1 {
                unsafe { ctx.inputs.read_sample(1, frame) }
            } else {
                input_l
            };
            let (out_l, out_r) = self.delay.process_sample(input_l, input_r);
            unsafe {
                ctx.outputs.write_sample(0, frame, out_l);
                if channels > 1 {
                    ctx.outputs.write_sample(1, frame, out_r);
                }
            }
        }
        if channels > 2 {
            for ch in 2..channels {
                for frame in 0..frames {
                    let sample = unsafe { ctx.inputs.read_sample(ch, frame) };
                    unsafe { ctx.outputs.write_sample(ch, frame, sample) };
                }
            }
        }
    }
}
