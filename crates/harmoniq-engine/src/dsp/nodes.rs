use harmoniq_dsp::biquad::Svf;
use harmoniq_dsp::delay::StereoDelay;
use harmoniq_dsp::gain::db_to_linear;
use harmoniq_dsp::pan::constant_power;
use harmoniq_dsp::saturator::soft_clip;
use harmoniq_dsp::smoothing::OnePole;

use crate::dsp::graph::{DspNode, NodeLatency, ProcessContext};
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
    }
}

pub struct LowpassNode {
    left: Svf,
    right: Svf,
    cutoff_hz: f32,
    resonance: f32,
    sample_rate: f32,
    current_cutoff: f32,
}

impl LowpassNode {
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

impl DspNode for LowpassNode {
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

pub struct SaturatorNode {
    drive: f32,
    makeup_db: f32,
    smoother: OnePole,
}

impl SaturatorNode {
    pub fn new(drive: f32, makeup_db: f32) -> Self {
        Self {
            drive: drive.max(0.0),
            makeup_db,
            smoother: OnePole::new(48_000.0, 5.0),
        }
    }
}

impl DspNode for SaturatorNode {
    fn prepare(&mut self, sr: f32, _max_block: u32, _in: u32, _out: u32) {
        self.smoother.set_time_ms(sr, 5.0);
        self.smoother.reset(self.drive);
    }

    fn param(&mut self, update: ParamUpdate) {
        match update.id {
            0 => {
                self.drive = update.value.max(0.0);
            }
            1 => {
                self.makeup_db = update.value;
            }
            _ => {}
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_>) {
        let frames = ctx.frames as usize;
        let channels = ctx.outputs.channels().min(ctx.inputs.channels()) as usize;
        let makeup = db_to_linear(self.makeup_db);
        for frame in 0..frames {
            let drive = self.smoother.next(self.drive);
            for ch in 0..channels {
                let sample = unsafe { ctx.inputs.read_sample(ch, frame) } * drive;
                let shaped = soft_clip(sample) * makeup;
                unsafe { ctx.outputs.write_sample(ch, frame, shaped) };
            }
        }
    }
}

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
