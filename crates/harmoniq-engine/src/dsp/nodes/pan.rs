use harmoniq_dsp::pan::constant_power;
use harmoniq_dsp::smoothing::OnePole;

use crate::dsp::graph::{DspNode, ProcessContext};
use crate::dsp::params::ParamUpdate;

pub struct PanNode {
    pan: f32,
    smoother: OnePole,
}

impl PanNode {
    pub fn new(initial_pan: f32) -> Self {
        Self {
            pan: initial_pan.clamp(-1.0, 1.0),
            smoother: OnePole::new(48_000.0, 5.0),
        }
    }
}

impl DspNode for PanNode {
    fn prepare(&mut self, sr: f32, _max_block: u32, _in: u32, _out: u32) {
        self.smoother.set_time_ms(sr, 5.0);
        self.smoother.reset(self.pan);
    }

    fn param(&mut self, update: ParamUpdate) {
        if update.id == 0 {
            self.pan = update.value.clamp(-1.0, 1.0);
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        let frames = ctx.frames as usize;
        let channels = ctx.outputs.channels().min(ctx.inputs.channels()) as usize;
        if channels == 0 {
            return;
        }
        for frame in 0..frames {
            let pan = self.smoother.next(self.pan);
            let (g_l, g_r) = constant_power(pan);
            let left = unsafe { ctx.inputs.read_sample(0, frame) } * g_l;
            if channels == 1 {
                unsafe { ctx.outputs.write_sample(0, frame, left) };
            } else {
                let right_in = unsafe { ctx.inputs.read_sample(1, frame) };
                let right = right_in * g_r;
                unsafe {
                    ctx.outputs.write_sample(0, frame, left);
                    ctx.outputs.write_sample(1, frame, right);
                }
                for ch in 2..channels {
                    let sample = unsafe { ctx.inputs.read_sample(ch, frame) };
                    unsafe { ctx.outputs.write_sample(ch, frame, sample) };
                }
            }
        }
    }
}
