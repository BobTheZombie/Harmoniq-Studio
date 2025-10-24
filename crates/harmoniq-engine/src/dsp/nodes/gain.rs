use harmoniq_dsp::gain::db_to_linear;
use harmoniq_dsp::smoothing::OnePole;
use harmoniq_dsp::{ChanMut, ChanRef};

use crate::dsp::graph::{DspNode, ProcessContext};
use crate::dsp::params::ParamUpdate;

pub struct GainNode {
    target_db: f32,
    smoother: OnePole,
    gains: Vec<f32>,
}

impl GainNode {
    pub fn new(initial_db: f32) -> Self {
        Self {
            target_db: initial_db,
            smoother: OnePole::new(48_000.0, 5.0),
            gains: Vec::new(),
        }
    }
}

impl DspNode for GainNode {
    fn prepare(&mut self, sr: f32, max_block: u32, _in: u32, _out: u32) {
        self.smoother.set_time_ms(sr, 5.0);
        self.smoother.reset(db_to_linear(self.target_db));
        self.gains.resize(max_block as usize, 0.0);
    }

    fn param(&mut self, update: ParamUpdate) {
        if update.id == 0 {
            self.target_db = update.value;
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        let frames = ctx.frames as usize;
        if self.gains.len() < frames {
            self.gains.resize(frames, 0.0);
        }
        let channels = ctx.outputs.channels().min(ctx.inputs.channels()) as usize;
        let target = db_to_linear(self.target_db);
        for frame in 0..frames {
            self.gains[frame] = self.smoother.next(target);
        }
        let gains = &self.gains[..frames];
        for ch in 0..channels {
            unsafe {
                let src = ctx.inputs.chan(ch);
                let mut dst = ctx.outputs.chan_mut(ch);
                apply_gain_curve(&mut dst, &src, gains);
            }
        }
        for ch in channels..ctx.outputs.channels() as usize {
            unsafe {
                let mut dst = ctx.outputs.chan_mut(ch);
                let frames = frames.min(dst.frames());
                for frame in 0..frames {
                    dst.write(frame, 0.0);
                }
            }
        }
    }
}

#[inline]
fn apply_gain_curve(dst: &mut ChanMut<'_>, src: &ChanRef<'_>, gains: &[f32]) {
    let frames = gains.len().min(src.frames()).min(dst.frames());

    #[cfg(feature = "simd")]
    {
        if let (Some(src_slice), Some(dst_slice)) =
            (unsafe { src.as_slice() }, unsafe { dst.as_mut_slice() })
        {
            harmoniq_dsp::simd::mul_buffers_to(
                &mut dst_slice[..frames],
                &src_slice[..frames],
                &gains[..frames],
            );
            return;
        }
    }

    for frame in 0..frames {
        let sample = unsafe { src.read(frame) };
        unsafe { dst.write(frame, sample * gains[frame]) };
    }
}
