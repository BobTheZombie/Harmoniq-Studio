use std::f32::consts::TAU;

pub const MAX_VOICES: usize = 8;
const WT_SIZE: usize = 2048;

/// A single synth voice. No heap allocations. Sine wavetable + envelopes.
#[derive(Clone)]
pub struct SubVoice {
    pub current_note: Option<u8>,
    pub age: u64, // increment per sample while active

    // oscillator state
    phase: f32,
    phase_inc: f32,
    target_inc: f32, // for glide

    // amp env (808-like: mainly decay)
    env_amp: f32,
    env_amp_coeff: f32,
    env_gate: bool,

    // pitch thump env
    env_thump: f32,
    env_thump_coeff: f32,

    // velocity
    velocity: f32,

    // wavetable
    wt: [f32; WT_SIZE],
}

impl SubVoice {
    pub fn new() -> Self {
        let mut wt = [0.0f32; WT_SIZE];
        for i in 0..WT_SIZE {
            let p = i as f32 / WT_SIZE as f32;
            wt[i] = (TAU * p).sin();
        }
        Self {
            current_note: None,
            age: 0,
            phase: 0.0,
            phase_inc: 0.0,
            target_inc: 0.0,
            env_amp: 0.0,
            env_amp_coeff: 0.0,
            env_gate: false,
            env_thump: 0.0,
            env_thump_coeff: 0.0,
            velocity: 1.0,
            wt,
        }
    }

    #[inline]
    pub fn reset(&mut self, _sr: f32) {
        self.current_note = None;
        self.age = 0;
        self.phase = 0.0;
        self.phase_inc = 0.0;
        self.target_inc = 0.0;
        self.env_amp = 0.0;
        self.env_amp_coeff = 0.0;
        self.env_gate = false;
        self.env_thump = 0.0;
        self.env_thump_coeff = 0.0;
        self.velocity = 1.0;
    }

    #[inline]
    pub fn is_on(&self) -> bool {
        self.current_note.is_some() && (self.env_amp > 1e-5 || self.env_gate)
    }

    pub fn note_on(
        &mut self,
        note: u8,
        vel: f32,
        sr: f32,
        decay_s: f32,
        _thump_st: f32,
        thump_decay_s: f32,
        glide_ms: f32,
    ) {
        self.current_note = Some(note);
        self.velocity = vel;
        self.env_gate = true;
        self.age = 0;

        // base frequency
        let freq = midi_to_hz(note as f32);
        let inc = freq / sr;

        // glide target inc
        self.target_inc = inc;
        let glide_s = (glide_ms * 0.001).max(0.0);
        if glide_s > 0.0 && self.phase_inc > 0.0 {
            let coeff = time_to_coeff(glide_s, sr);
            // simple lag toward target
            self.phase_inc = self.phase_inc + coeff * (self.target_inc - self.phase_inc);
        } else {
            self.phase_inc = inc;
        }

        // amp env: set decay coefficient
        self.env_amp = 1.0;
        self.env_amp_coeff = time_to_coeff(decay_s.max(1e-4), sr);

        // thump env
        self.env_thump = 1.0;
        self.env_thump_coeff = time_to_coeff(thump_decay_s.max(1e-4), sr);
        // phase remains
    }

    #[inline]
    pub fn note_off(&mut self) {
        // 808 behavior: continue decay, gate false
        self.env_gate = false;
    }

    #[inline]
    pub fn kill(&mut self) {
        self.current_note = None;
        self.env_gate = false;
        self.env_amp = 0.0;
        self.env_thump = 0.0;
    }

    /// Process one sample and return mono signal.
    pub fn process_one(
        &mut self,
        sr: f32,
        decay_s: f32,
        thump_st: f32,
        thump_decay_s: f32,
        glide_ms: f32,
        vel_sens: f32,
    ) -> f32 {
        // update decay coeffs if host automates them
        self.env_amp_coeff = time_to_coeff(decay_s.max(1e-4), sr);
        self.env_thump_coeff = time_to_coeff(thump_decay_s.max(1e-4), sr);

        // advance envelopes
        self.env_amp *= 1.0 - self.env_amp_coeff;
        if !self.env_gate && self.env_amp < 1e-5 {
            // finish voice
            self.current_note = None;
            return 0.0;
        }
        self.env_thump *= 1.0 - self.env_thump_coeff;

        // glide toward target phase_inc
        let glide_s = (glide_ms * 0.001).max(0.0);
        if glide_s > 0.0 {
            let gcoeff = time_to_coeff(glide_s, sr);
            self.phase_inc += gcoeff * (self.target_inc - self.phase_inc);
        } else {
            self.phase_inc = self.target_inc;
        }

        // pitch thump: add semitone offset decaying to zero
        let thump_off_st = thump_st * self.env_thump;
        let thump_ratio = semitone_ratio(thump_off_st);
        let inc = (self.phase_inc * thump_ratio).min(0.49); // safety clamp Nyquist

        // oscillator
        self.phase = (self.phase + inc) % 1.0;
        let s = self.wt_lookup(self.phase);

        // amp with velocity
        let vels = 1.0 + vel_sens * (self.velocity - 1.0);
        let out = s * self.env_amp * vels.max(0.0);

        self.age = self.age.saturating_add(1);
        out
    }

    #[inline(always)]
    fn wt_lookup(&self, phase: f32) -> f32 {
        let x = (phase * WT_SIZE as f32).floor() as usize;
        let x2 = (x + 1) & (WT_SIZE - 1);
        let frac = (phase * WT_SIZE as f32) - x as f32;
        let a = self.wt[x];
        let b = self.wt[x2];
        a + (b - a) * frac
    }
}

#[inline(always)]
fn midi_to_hz(n: f32) -> f32 {
    // A4 = 440 at note 69
    440.0 * 2.0f32.powf((n - 69.0) / 12.0)
}

#[inline(always)]
fn time_to_coeff(t_s: f32, sr: f32) -> f32 {
    // convert a time constant to per-sample 1-pole coefficient to reach ~63% in t_s
    // y[n] = (1 - a) * y[n-1], a = 1 - exp(-1/(t*sr))
    if t_s <= 0.0 {
        return 1.0;
    }
    1.0 - (-1.0 / (t_s * sr)).exp()
}

#[inline(always)]
fn semitone_ratio(st: f32) -> f32 {
    (st / 12.0).exp2()
}
