use harmoniq_dsp::gain::db_to_linear;
use harmoniq_dsp::smoothing::OnePole;

use crate::dsp::graph::{DspNode, ProcessContext};
use crate::dsp::params::ParamUpdate;

pub struct GainNode {
    target_db: f32,
    smoother: OnePole,
}

impl GainNode {
    pub fn new(initial_db: f32) -> Self {
        Self {
            target_db: initial_db,
            smoother: OnePole::new(48_000.0, 5.0),
        }
    }
}

impl DspNode for GainNode {
    fn prepare(&mut self, sr: f32, _max_block: u32, _in: u32, _out: u32) {
        self.smoother.set_time_ms(sr, 5.0);
        self.smoother.reset(db_to_linear(self.target_db));
    }

    fn param(&mut self, update: ParamUpdate) {
        if update.id == 0 {
            self.target_db = update.value;
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        let frames = ctx.frames as usize;
        let channels = ctx.outputs.channels().min(ctx.inputs.channels()) as usize;
        let target = db_to_linear(self.target_db);
        for frame in 0..frames {
            let gain = self.smoother.next(target);
            for ch in 0..channels {
                let sample = unsafe { ctx.inputs.read_sample(ch, frame) };
                unsafe { ctx.outputs.write_sample(ch, frame, sample * gain) };
            }
        }
        for ch in channels..ctx.outputs.channels() as usize {
            for frame in 0..frames {
                unsafe { ctx.outputs.write_sample(ch, frame, 0.0) };
            }
        }
    }
}
