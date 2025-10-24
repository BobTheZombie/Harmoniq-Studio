use harmoniq_dsp::biquad::Svf;

use crate::dsp::graph::{DspNode, ProcessContext};
use crate::dsp::params::ParamUpdate;

pub struct SvfLowpassNode {
    left: Svf,
    right: Svf,
    cutoff_hz: f32,
    resonance: f32,
    sample_rate: f32,
    current_cutoff: f32,
}

impl SvfLowpassNode {
    pub fn new(cutoff: f32, resonance: f32) -> Self {
        let cutoff = cutoff.max(20.0);
        let resonance = resonance.max(0.05);
        Self {
            left: Svf::lowpass(48_000.0, cutoff, resonance),
            right: Svf::lowpass(48_000.0, cutoff, resonance),
            cutoff_hz: cutoff,
            resonance,
            sample_rate: 48_000.0,
            current_cutoff: cutoff,
        }
    }

    fn refresh_filters(&mut self) {
        self.left
            .set_lowpass(self.sample_rate, self.cutoff_hz, self.resonance);
        self.right
            .set_lowpass(self.sample_rate, self.cutoff_hz, self.resonance);
        self.current_cutoff = self.cutoff_hz;
    }
}

impl DspNode for SvfLowpassNode {
    fn prepare(&mut self, sr: f32, _max_block: u32, _in: u32, _out: u32) {
        self.sample_rate = sr.max(1.0);
        self.refresh_filters();
    }

    fn param(&mut self, update: ParamUpdate) {
        match update.id {
            0 => {
                self.cutoff_hz = update.value.max(20.0);
            }
            1 => {
                self.resonance = update.value.max(0.05);
            }
            _ => {}
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        if (self.cutoff_hz - self.current_cutoff).abs() > 0.5 {
            self.refresh_filters();
        }
        let frames = ctx.frames as usize;
        let channels = ctx.outputs.channels().min(ctx.inputs.channels()) as usize;
        match channels {
            0 => {}
            1 => {
                for frame in 0..frames {
                    let input = unsafe { ctx.inputs.read_sample(0, frame) };
                    let out = self.left.process(input);
                    unsafe { ctx.outputs.write_sample(0, frame, out) };
                }
            }
            _ => {
                for frame in 0..frames {
                    let l = unsafe { ctx.inputs.read_sample(0, frame) };
                    let r = unsafe { ctx.inputs.read_sample(1, frame) };
                    let out_l = self.left.process(l);
                    let out_r = self.right.process(r);
                    unsafe {
                        ctx.outputs.write_sample(0, frame, out_l);
                        ctx.outputs.write_sample(1, frame, out_r);
                    }
                }
                for ch in 2..channels {
                    for frame in 0..frames {
                        let sample = unsafe { ctx.inputs.read_sample(ch, frame) };
                        unsafe { ctx.outputs.write_sample(ch, frame, sample) };
                    }
                }
            }
        }
    }
}
