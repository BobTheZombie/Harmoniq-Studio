use super::env::AdsrEnvelope;
use super::filter::LadderFilter;
use super::osc::Oscillator;

#[derive(Clone, Copy, Debug)]
pub struct EnvelopeSettings {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceParams {
    pub blend: f32,
    pub sub_level: f32,
    pub detune_cents: f32,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_env_amount: f32,
    pub velocity_to_cutoff: bool,
    pub filter_env_depth: f32,
    pub lfo_value: f32,
    pub lfo_pitch_amount: f32,
    pub lfo_cutoff_amount: f32,
    pub lfo_amp_amount: f32,
    pub pitch_bend_semitones: f32,
    pub velocity_amp_scale: f32,
}

pub struct Voice {
    sample_rate: f32,
    pub active: bool,
    note: u8,
    velocity: f32,
    current_freq: f32,
    target_freq: f32,
    glide_step: f32,
    glide_samples: f32,
    sine: Oscillator,
    saw: Oscillator,
    sub: Oscillator,
    amp_env: AdsrEnvelope,
    filter_env: AdsrEnvelope,
    filter: LadderFilter,
    released: bool,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            active: false,
            note: 0,
            velocity: 0.0,
            current_freq: 0.0,
            target_freq: 0.0,
            glide_step: 0.0,
            glide_samples: 0.0,
            sine: Oscillator::new(),
            saw: Oscillator::new(),
            sub: Oscillator::new(),
            amp_env: AdsrEnvelope::new(sample_rate),
            filter_env: AdsrEnvelope::new(sample_rate),
            filter: LadderFilter::new(sample_rate),
            released: false,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.amp_env.set_sample_rate(self.sample_rate);
        self.filter_env.set_sample_rate(self.sample_rate);
        self.filter.set_sample_rate(self.sample_rate);
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.velocity = 0.0;
        self.current_freq = 0.0;
        self.target_freq = 0.0;
        self.glide_step = 0.0;
        self.glide_samples = 0.0;
        self.sine.reset();
        self.saw.reset();
        self.sub.reset();
        self.amp_env.reset();
        self.filter_env.reset();
        self.filter.reset();
        self.released = false;
    }

    pub fn apply_envelopes(&mut self, amp: EnvelopeSettings, filter: EnvelopeSettings) {
        self.amp_env
            .set_params(amp.attack, amp.decay, amp.sustain, amp.release);
        self.filter_env
            .set_params(filter.attack, filter.decay, filter.sustain, filter.release);
    }

    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        freq_hz: f32,
        filter_cutoff: f32,
        filter_resonance: f32,
        glide_time: f32,
        amp_env: EnvelopeSettings,
        filter_env: EnvelopeSettings,
    ) {
        self.note = note;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.apply_envelopes(amp_env, filter_env);
        self.current_freq = freq_hz;
        self.target_freq = freq_hz;
        self.glide_step = 0.0;
        self.glide_samples = 0.0;
        self.sine.reset();
        self.saw.reset();
        self.sub.reset();
        self.amp_env.trigger();
        self.filter_env.trigger();
        self.filter.set_params(filter_cutoff, filter_resonance);
        self.released = false;
        self.active = true;
    }

    pub fn legato_note(
        &mut self,
        note: u8,
        velocity: f32,
        freq_hz: f32,
        filter_cutoff: f32,
        filter_resonance: f32,
        glide_time: f32,
        amp_env: EnvelopeSettings,
        filter_env: EnvelopeSettings,
    ) {
        self.note = note;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.apply_envelopes(amp_env, filter_env);
        self.start_glide(freq_hz, glide_time);
        self.filter.set_params(filter_cutoff, filter_resonance);
        self.released = false;
        self.active = true;
    }

    pub fn note_off(&mut self) {
        if self.active {
            self.amp_env.release();
            self.filter_env.release();
            self.released = true;
        }
    }

    pub fn finished(&self) -> bool {
        !self.amp_env.is_active() && self.released
    }

    fn start_glide(&mut self, target_freq: f32, glide_time: f32) {
        self.target_freq = target_freq;
        let time = glide_time.max(0.0);
        if time <= f32::EPSILON {
            self.current_freq = target_freq;
            self.glide_samples = 0.0;
            self.glide_step = 0.0;
        } else {
            self.glide_samples = (time * self.sample_rate).max(1.0);
            self.glide_step = (self.target_freq - self.current_freq) / self.glide_samples;
        }
    }

    fn update_frequency(&mut self) {
        if self.glide_samples > 0.0 {
            self.current_freq += self.glide_step;
            self.glide_samples -= 1.0;
            if self.glide_samples <= 0.0 {
                self.current_freq = self.target_freq;
                self.glide_samples = 0.0;
            }
        } else {
            self.current_freq = self.target_freq;
        }
    }

    pub fn render(&mut self, params: &VoiceParams) -> f32 {
        if !self.active {
            return 0.0;
        }

        self.update_frequency();

        let pitch_bend_ratio = (2.0f32).powf(params.pitch_bend_semitones / 12.0);
        let base_freq = (self.current_freq * pitch_bend_ratio).clamp(0.0, self.sample_rate * 0.45);
        let lfo_pitch = params.lfo_value * params.lfo_pitch_amount;
        let pitch_mod_ratio = (2.0f32).powf(lfo_pitch / 12.0);
        let freq = (base_freq * pitch_mod_ratio).clamp(20.0, self.sample_rate * 0.45);
        let detune_ratio = (2.0f32).powf(params.detune_cents / 1200.0);
        let sine = self.sine.advance_sine(freq, self.sample_rate);
        let saw = self.saw.advance_saw(freq * detune_ratio, self.sample_rate);
        let sub = self.sub.advance_sine(freq * 0.5, self.sample_rate);

        let mut sample = sine * (1.0 - params.blend) + saw * params.blend;
        sample += sub * params.sub_level;

        let filter_env = self.filter_env.next_sample() * params.filter_env_depth;
        let mut cutoff = params.filter_cutoff * (1.0 + params.filter_env_amount * filter_env * 3.0);
        if params.velocity_to_cutoff {
            cutoff *= 0.5 + self.velocity * 0.75;
        }
        cutoff *= 1.0 + params.lfo_cutoff_amount * params.lfo_value;
        cutoff = cutoff.clamp(20.0, 20_000.0);
        self.filter
            .set_params(cutoff.min(self.sample_rate * 0.45), params.filter_resonance);

        let signal = self.filter.process(sample);

        let amp_env = self.amp_env.next_sample();
        let amp_mod = 1.0 + params.lfo_amp_amount * params.lfo_value;
        let mut gain = amp_env * self.velocity * params.velocity_amp_scale * amp_mod;
        gain = gain.clamp(0.0, 1.5);
        let output = signal * gain;

        if self.finished() {
            self.reset();
        }

        output
    }
}
